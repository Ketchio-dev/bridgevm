//! CPU kernel and bounded disk write+fsync micro-benchmarks with their reports.

use anyhow::Result;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

/// Number of compute-kernel iterations folded between each wall-clock deadline
/// check. Small enough that the benchmark stops promptly once the budget is
/// spent, large enough that the `Instant::now()` overhead stays negligible.
pub(crate) const BENCHMARK_KERNEL_CHUNK: u64 = 4_096;

/// Fixed, small payload for the optional disk micro-benchmark. Bounded by
/// construction so the guest never writes unbounded data to its own disk.
pub(crate) const BENCHMARK_DISK_BYTES: usize = 256 * 1024;

/// Pure, deterministic compute kernel: an FNV-1a-style integer hash fold over
/// `iterations` steps starting from `seed`. It performs a fixed amount of work
/// per iteration and returns the same value for the same `(seed, iterations)`
/// input on every platform, so it is unit-testable independently of timing and
/// usable as a CPU-load generator. No allocation, no I/O, no unbounded loops.
pub(crate) fn benchmark_kernel(seed: u64, iterations: u64) -> u64 {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut state = seed ^ 0xcbf2_9ce4_8422_2325;
    let mut i = 0u64;
    while i < iterations {
        // Mix the counter in and fold; wrapping ops keep this total and
        // deterministic regardless of overflow.
        state ^= i;
        state = state.wrapping_mul(FNV_PRIME);
        state = state.rotate_left(13) ^ (state >> 7);
        i += 1;
    }
    state
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CpuBenchmarkReport {
    pub(crate) iterations: u64,
    pub(crate) elapsed_millis: u64,
    pub(crate) ops_per_sec: u64,
    pub(crate) checksum: u64,
}

/// Run the pure kernel in fixed-size chunks until the wall-clock `budget` is
/// spent, then report iterations completed, elapsed time, and a derived
/// ops/sec figure. Bounded by `budget` (the caller clamps it to a hard maximum)
/// and by the chunked deadline check; it never loops unbounded and never
/// allocates.
pub(crate) fn run_cpu_benchmark(budget: Duration) -> CpuBenchmarkReport {
    let start = Instant::now();
    let deadline = start + budget;
    let mut iterations: u64 = 0;
    let mut checksum: u64 = 0;
    // Always run at least one chunk so a tiny budget still yields a real figure.
    loop {
        checksum = benchmark_kernel(checksum, BENCHMARK_KERNEL_CHUNK);
        iterations = iterations.saturating_add(BENCHMARK_KERNEL_CHUNK);
        if Instant::now() >= deadline {
            break;
        }
    }
    let elapsed = start.elapsed();
    let elapsed_millis = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
    let elapsed_secs = elapsed.as_secs_f64();
    let ops_per_sec = if elapsed_secs > 0.0 {
        (iterations as f64 / elapsed_secs)
            .round()
            .min(u64::MAX as f64) as u64
    } else {
        0
    };
    CpuBenchmarkReport {
        iterations,
        elapsed_millis,
        ops_per_sec,
        checksum,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiskBenchmarkReport {
    pub(crate) bytes_written: usize,
    pub(crate) elapsed_millis: u64,
    pub(crate) mib_per_sec: u64,
}

/// Tiny disk write+fsync micro-benchmark: write a fixed, small buffer to a
/// uniquely-named temp file in `dir`, fsync it, measure, then always remove the
/// file. The payload size is a compile-time constant, so this never writes
/// unbounded data; a write/sync error is surfaced to the caller as `Err`.
pub(crate) fn run_disk_benchmark(dir: &Path) -> Result<DiskBenchmarkReport, String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("failed to create benchmark scratch dir: {error}"))?;
    let micros = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH_FOR_BENCH)
        .map(|since| since.as_micros())
        .unwrap_or(0);
    let path = dir.join(format!(
        ".bridgevm-bench-{}-{micros}.tmp",
        std::process::id()
    ));
    let payload = vec![0xA5u8; BENCHMARK_DISK_BYTES];

    let start = Instant::now();
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(&payload)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();
    let elapsed = start.elapsed();
    // Always remove the temp file, whether or not the write succeeded.
    let _ = fs::remove_file(&path);
    result.map_err(|error| format!("benchmark disk write failed: {error}"))?;

    let elapsed_millis = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
    let elapsed_secs = elapsed.as_secs_f64();
    let mib = BENCHMARK_DISK_BYTES as f64 / (1024.0 * 1024.0);
    let mib_per_sec = if elapsed_secs > 0.0 {
        (mib / elapsed_secs).round().min(u64::MAX as f64) as u64
    } else {
        0
    };
    Ok(DiskBenchmarkReport {
        bytes_written: BENCHMARK_DISK_BYTES,
        elapsed_millis,
        mib_per_sec,
    })
}

/// Epoch constant for naming the benchmark scratch file. Aliased so the disk
/// benchmark does not depend on the test-only `UNIX_EPOCH` import.
use std::time::UNIX_EPOCH as UNIX_EPOCH_FOR_BENCH;
