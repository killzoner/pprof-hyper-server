[![continuous-integration](https://github.com/killzoner/pprof-hyper-server/actions/workflows/continuous-integration.yml/badge.svg)](https://github.com/killzoner/pprof-hyper-server/actions/workflows/continuous-integration.yml)

# pprof-hyper-server

> **A minimal pprof server implementation using `hyper` without runtime dependency**

## About

Easily CPU/memory profile your Rust application with pprof.

For more details, see:

- [examples](https://github.com/killzoner/pprof-hyper-server/tree/master/examples)

You most likely need a linux-ish machine for it to work (current msvc is not supported for both cpu and memory profiling).

## Basic API usage with pprof client

Install [pprof](https://github.com/google/pprof) client or use the one from Golang toolchain.

With Golang toolchain:

```bash
go tool pprof --http=: http://localhost:6060/debug/pprof/profile # CPU profiling
go tool pprof --http=: http://localhost:6060/debug/pprof/allocs # memory profiling
```

## Related projects

- Project <https://github.com/tikv/pprof-rs> used internally for CPU profiling.
- Project <https://github.com/polarsignals/rust-jemalloc-pprof> used internally for memory profiling using Jemalloc allocator.
