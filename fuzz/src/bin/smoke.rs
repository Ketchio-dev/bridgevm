//! Run every fuzz target over the checked-in corpus and a bounded set of
//! deterministic mutations, on the pinned stable toolchain.
//!
//! `cargo fuzz` needs nightly and is not installed everywhere, so without this
//! the corpus would only ever run on a machine that has it -- which makes the
//! seeds decoration rather than regression tests. This runs the same target
//! bodies, so a seed that reproduces a crash reproduces it here too.
//!
//! It is a smoke test, not a fuzzing campaign: it proves the targets are
//! wired up and the corpus is clean, and finds nothing new on its own.

use std::path::Path;

fn main() {
    let corpus_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut inputs = 0usize;
    let mut targets = 0usize;

    for (name, body) in bridgevm_fuzz::TARGETS {
        targets += 1;
        let dir = corpus_root.join(name);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("FAIL: corpus for {name} is unreadable: {error}");
                std::process::exit(1);
            }
        };

        let mut seeds = 0usize;
        for entry in entries.flatten() {
            let data = match std::fs::read(entry.path()) {
                Ok(data) => data,
                Err(error) => {
                    eprintln!("FAIL: cannot read {}: {error}", entry.path().display());
                    std::process::exit(1);
                }
            };
            seeds += 1;
            body(&data);
            inputs += 1;
            for mutated in mutations(&data) {
                body(&mutated);
                inputs += 1;
            }
        }

        if seeds == 0 {
            // An empty corpus would let this pass while testing nothing.
            eprintln!("FAIL: target {name} has no corpus seeds");
            std::process::exit(1);
        }
        println!("  {name}: {seeds} seeds");
    }

    println!("PASS: fuzz smoke, {targets} targets, {inputs} inputs, 0 crashes");
}

/// Deterministic mutations. Bounded and reproducible on purpose: a random
/// smoke test that fails once and never again is worse than none.
fn mutations(seed: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();

    // Truncations, including to nothing: length checks are the usual bug.
    out.push(Vec::new());
    if !seed.is_empty() {
        out.push(seed[..seed.len() / 2].to_vec());
        out.push(seed[..seed.len() - 1].to_vec());
    }

    // Bit flips in the first bytes, where type and length fields live.
    for index in 0..seed.len().min(8) {
        for bit in [0u8, 7] {
            let mut mutated = seed.to_vec();
            mutated[index] ^= 1 << bit;
            out.push(mutated);
        }
    }

    // Saturated fields: all-ones is the classic length-overflow input.
    if !seed.is_empty() {
        out.push(vec![0xff; seed.len()]);
        out.push(vec![0x00; seed.len()]);
        let mut extended = seed.to_vec();
        extended.extend_from_slice(&[0xff; 32]);
        out.push(extended);
    }

    out
}
