use anyhow::Result;
use clap::Parser;
use pprof_hyper_server::Config;
use tokio::{
    task::{self, JoinHandle},
    time::{self, Duration},
};

/// Global jemalloc allocator config for memory profiling.
/// Define this at the root of your app.
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Global jemalloc allocator config for memory profiling.
/// Define this at the root of your app.
#[allow(non_upper_case_globals)]
#[unsafe(export_name = "malloc_conf")]
pub static malloc_conf: &[u8] = b"prof:true,prof_active:true,lg_prof_sample:19\0";

#[derive(clap::Args, Debug, Clone)]
pub struct Options {
    #[arg(
        env,
        long,
        help = "Bind address for pprof server",
        required = false,
        default_value = "[::]:6060"
    )]
    pub bind_address: std::net::SocketAddr,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about)]
struct Cli {
    #[command(flatten)]
    pprof: Options,
}

// cargo run --example server
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // allocate some memory and use the result (compiler could remove completely the unused code / allocation otherwise)
    let v = allocate_memory();

    println!("{:?}", v.len());

    // create a CPU intensive task
    let t1: JoinHandle<_> = task::spawn(async move {
        let prime_numbers = prepare_prime_numbers();
        let mut interval = time::interval(Duration::from_millis(100));

        loop {
            interval.tick().await;

            for i in 2..50000 {
                let _ = is_prime_number(i, &prime_numbers);
            }
        }
    });

    let t2: JoinHandle<_> = task::spawn(async move {
        pprof_hyper_server::serve(cli.pprof.bind_address, Config::default())
            .await
            .unwrap();
    });

    let _ = tokio::join!(t1, t2);

    Ok(())
}

#[inline(never)]
// from https://github.com/polarsignals/rust-jemalloc-pprof/examples
fn allocate_memory() -> Vec<i32> {
    let mut v = vec![];
    for i in 0..3000000 {
        v.push(i); // ~16MB
    }

    v
}

// from https://github.com/tikv/pprof-rs/blob/master/examples
#[inline(never)]
fn is_prime_number(v: usize, prime_numbers: &[usize]) -> bool {
    if v < 10000 {
        let r = prime_numbers.binary_search(&v);
        return r.is_ok();
    }

    for n in prime_numbers {
        if v % n == 0 {
            return false;
        }
    }

    true
}

// from https://github.com/tikv/pprof-rs/blob/master/examples
#[inline(never)]
fn prepare_prime_numbers() -> Vec<usize> {
    // bootstrap: Generate a prime table of 0..10000
    let mut prime_number_table: [bool; 10000] = [true; 10000];
    prime_number_table[0] = false;
    prime_number_table[1] = false;
    for i in 2..10000 {
        if prime_number_table[i] {
            let mut v = i * 2;
            while v < 10000 {
                prime_number_table[v] = false;
                v += i;
            }
        }
    }
    let mut prime_numbers = vec![];
    for (i, exist) in prime_number_table.iter().enumerate().skip(2) {
        if *exist {
            prime_numbers.push(i);
        }
    }
    prime_numbers
}
