#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::print_stderr)]
#![warn(clippy::print_stdout)]

use anyhow::Result;
use async_channel::bounded;
use async_io::Async;
use futures_lite::future;
use http_body_util::Full;
use hyper::{
    Method, Request, Response, StatusCode, Uri,
    body::{Bytes, Incoming},
    service::service_fn,
};
use smol_hyper::rt::FuturesIo;
use std::{
    borrow::Cow,
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

const MAX_CONCURRENT_REQUESTS: usize = 2; // 1 cpu + 1 mem
const NOT_FOUND: &[u8] = "Not Found".as_bytes();

/// Config allows customizing global pprof config.
#[derive(Default, Clone, Debug)]
pub struct Config<'a> {
    /// Defaults to pprof_cpu::PPROF_BLOCKLIST.
    pub pprof_blocklist: Option<&'a [&'a str]>,
    /// Defaults to pprof_cpu::PPROF_DEFAULT_SECONDS.
    pub pprof_default_seconds: Option<i32>,
    /// Defaults to pprof_cpu::PPROF_DEFAULT_SAMPLING.
    pub pprof_default_sampling: Option<i32>,
}

#[cfg(all(feature = "pprof_cpu", not(target_env = "msvc")))]
mod pprof_cpu {
    pub const PPROF_BLOCKLIST: &[&str; 4] = &["libc", "libgcc", "pthread", "vdso"];
    pub const PPROF_DEFAULT_SECONDS: i32 = 30; // same as golang pprof
    pub const PPROF_DEFAULT_SAMPLING: i32 = 99;
}

struct Task<'a> {
    client: Async<TcpStream>,
    config: Arc<Config<'a>>,
}

impl Task<'_> {
    /// Handle a new client.
    async fn handle_client(self) -> Result<()> {
        hyper::server::conn::http1::Builder::new()
            .serve_connection(
                FuturesIo::new(&self.client),
                service_fn(|req| self.serve(req)),
            )
            .await
            .unwrap_or_default(); // don't use ? otherwise early connection close errors are propagated

        Ok(())
    }

    async fn serve(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
        match (req.method(), req.uri().path()) {
            (&Method::GET, "/debug/pprof/allocs" | "/debug/pprof/heap") => {
                self.memory_profile().await
            }
            (&Method::GET, "/debug/pprof/profile") => self.cpu_profile(req).await,
            _ => not_found(),
        }
    }
}

impl Task<'_> {
    #[cfg(all(feature = "pprof_cpu", not(target_env = "msvc")))]
    async fn cpu_profile(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
        use crate::pprof_cpu::*;
        use async_io::Timer;
        use pprof::{ProfilerGuardBuilder, protos::Message};
        use std::time::Duration;

        let params = get_params(req.uri());

        let profile_seconds = parse_i32_params(
            &params,
            "seconds",
            self.config
                .pprof_default_seconds
                .unwrap_or(PPROF_DEFAULT_SECONDS),
        );
        let profile_sampling = parse_i32_params(
            &params,
            "sampling",
            self.config
                .pprof_default_sampling
                .unwrap_or(PPROF_DEFAULT_SAMPLING),
        );

        let blocklist = self.config.pprof_blocklist.unwrap_or(PPROF_BLOCKLIST);

        let guard = ProfilerGuardBuilder::default()
            .frequency(profile_sampling)
            .blocklist(blocklist)
            .build()?;

        Timer::after(Duration::from_secs(profile_seconds.try_into()?)).await;

        let profile = guard.report().build()?.pprof()?;

        let mut content = Vec::new();
        profile.encode(&mut content)?;

        Ok(Response::new(Full::new(Bytes::from(content))))
    }

    #[cfg(any(not(feature = "pprof_cpu"), target_env = "msvc"))]
    async fn cpu_profile(&self, _: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
        not_found()
    }

    #[cfg(all(feature = "pprof_heap", not(target_env = "msvc")))]
    async fn memory_profile(&self) -> Result<Response<Full<Bytes>>> {
        let prof_ctl = jemalloc_pprof::PROF_CTL.as_ref();

        match prof_ctl {
            None => Err(anyhow::anyhow!("heap profiling not activated")),
            Some(prof_ctl) => {
                let mut prof_ctl = prof_ctl.lock().await;

                if !prof_ctl.activated() {
                    return Err(anyhow::anyhow!("heap profiling not activated"));
                }

                let pprof = prof_ctl.dump_pprof()?;

                Ok(Response::new(Full::new(Bytes::from(pprof))))
            }
        }
    }

    #[cfg(any(not(feature = "pprof_heap"), target_env = "msvc"))]
    async fn memory_profile(&self) -> Result<Response<Full<Bytes>>> {
        not_found()
    }
}

#[allow(dead_code)]
fn get_params<'a>(uri: &'a Uri) -> HashMap<Cow<'a, str>, Cow<'a, str>> {
    let params: HashMap<Cow<'_, str>, Cow<'_, str>> = uri
        .query()
        .map(|v| form_urlencoded::parse(v.as_bytes()).collect())
        .unwrap_or_default();

    params
}

#[allow(dead_code)]
fn parse_i32_params<'a>(
    params: &'a HashMap<Cow<'a, str>, Cow<'a, str>>,
    name: &str,
    default: i32,
) -> i32 {
    params
        .get(name)
        .and_then(|e| e.parse::<i32>().ok())
        .unwrap_or(default)
}

fn not_found() -> Result<Response<Full<Bytes>>> {
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from(NOT_FOUND)))
        .unwrap_or_default())
}

/// Listens for incoming connections and serves them under pprof HTTP API.
pub async fn serve<'a>(bind_address: SocketAddr, config: Config<'a>) -> Result<()> {
    let listener = Async::<TcpListener>::bind(bind_address)?;
    let (s, r) = bounded::<Task>(MAX_CONCURRENT_REQUESTS);
    let config = Arc::new(config);

    loop {
        // stack max MAX_CONCURRENT_REQUESTS requests, prefering stacking than answering to them.
        // if we cannot stack anymore, drop the connection and other pending requests.
        // we don't need a multi threaded server to serve pprof server, but don't want it to be a source of DDOS.
        future::or(
            async {
                // Wait for a new client.
                let listener = listener.accept().await;
                if let Ok((client, _)) = listener {
                    let task = Task {
                        client,
                        config: config.clone(),
                    };

                    // we ignore the potential error as it would mean we should drop the connection if channel is full.
                    let _ = s.try_send(task);
                }
            },
            async {
                if let Ok(task) = r.recv().await {
                    task.handle_client().await.unwrap_or_default();
                }
            },
        )
        .await;
    }
}
