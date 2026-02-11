// cargo run --example heap_profile_diff
#[cfg(not(target_env = "msvc"))]
fn main() -> anyhow::Result<()> {
    app::run()
}

#[cfg(target_env = "msvc")]
fn main() {
    eprintln!("heap_profile_diff is not supported on msvc");
}

#[cfg(not(target_env = "msvc"))]
mod app {
    use anyhow::Result;

    #[global_allocator]
    static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

    // Override the system allocator even outside Rust code.
    // See https://github.com/tikv/jemallocator/blob/0.6.1/jemalloc-sys/README.md?plain=1#L91.
    use tikv_jemalloc_sys as _;

    #[allow(non_upper_case_globals)]
    #[unsafe(export_name = "malloc_conf")]
    pub static malloc_conf: &[u8] = b"prof:true,prof_active:true,lg_prof_sample:19\0";

    pub fn run() -> Result<()> {
        let prof_ctl = jemalloc_pprof::PROF_CTL
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("heap profiling not activated"))?;

        // --- set up the "before" state ---
        let mut buffers = workload::allocate_buffers(10); // 10 × 1 MiB  (will be augmented)
        let cache = workload::allocate_cache(); //  5 × 1 MiB  (will be freed)

        let before = {
            let mut ctl = prof_ctl.try_lock()?;
            ctl.dump_pprof()?
        };

        // --- mutate between snapshots ---
        drop(cache); // removed allocations
        buffers.extend(workload::allocate_buffers(5)); // augmented (same stack frame)
        let _new = workload::allocate_new_feature(); // new allocations

        let after = {
            let mut ctl = prof_ctl.try_lock()?;
            ctl.dump_pprof()?
        };

        // --- write snapshots ---
        let out_dir = std::env::temp_dir().join("pprof_heap_diff");
        std::fs::create_dir_all(&out_dir)?;
        std::fs::write(out_dir.join("before.pb.gz"), &before)?;
        std::fs::write(out_dir.join("after.pb.gz"), &after)?;

        println!("Wrote profiles to {}/", out_dir.display());
        println!();
        println!("Inspect the diff with:");
        println!(
            "  go tool pprof -http=:8080 -diff_base={} {}",
            out_dir.join("before.pb.gz").display(),
            out_dir.join("after.pb.gz").display()
        );

        Ok(())
    }

    /// Simulated workload functions. Each allocation path has a unique leaf
    /// function so the call stacks are clearly distinguishable in pprof.
    mod workload {
        #[inline(never)]
        pub fn allocate_buffers(count: usize) -> Vec<Vec<u8>> {
            (0..count).map(|_| buffer_1mib()).collect()
        }

        #[inline(never)]
        fn buffer_1mib() -> Vec<u8> {
            vec![0u8; 1_048_576]
        }

        #[inline(never)]
        pub fn allocate_cache() -> Vec<Vec<u8>> {
            (0..5).map(|_| cache_entry_1mib()).collect()
        }

        #[inline(never)]
        fn cache_entry_1mib() -> Vec<u8> {
            vec![0u8; 1_048_576]
        }

        #[inline(never)]
        pub fn allocate_new_feature() -> Vec<Vec<u8>> {
            (0..8).map(|_| feature_entry_1mib()).collect()
        }

        #[inline(never)]
        fn feature_entry_1mib() -> Vec<u8> {
            vec![0u8; 1_048_576]
        }
    }
}
