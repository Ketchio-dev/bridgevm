//! `--launch-spec`: the typed product entry point.
//!
//! Reads a versioned JSON manifest (or stdin for `-`), hands it to
//! `bridgevm-hvf-runtime` for validation, and reports what was accepted.
//! This is the replacement for `--launch`, which shells out to a repository
//! script; here nothing is executed through a shell and release builds
//! refuse manifests that point into a source repository.

use anyhow::{bail, Context, Result};
use bridgevm_hvf_runtime::LaunchManifest;
use std::io::Read;

/// Release builds enforce product policy; debug builds are evidence
/// harnesses and may run from a repository checkout.
const PRODUCT_POLICY: bool = !cfg!(debug_assertions);

pub(crate) fn run_launch_spec(spec: &str) -> Result<()> {
    let text = if spec == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("read launch manifest from stdin")?;
        text
    } else {
        std::fs::read_to_string(spec).with_context(|| format!("read launch manifest {spec}"))?
    };
    let manifest = match LaunchManifest::parse(&text, PRODUCT_POLICY) {
        Ok(manifest) => manifest,
        Err(error) => bail!("launch manifest rejected: {error}"),
    };
    // Lifecycle wiring (VmBuilder/VmRuntime) follows in the next R1 slice;
    // until then this validates and reports, so the manifest contract can be
    // exercised end to end from the CLI without a VM.
    println!(
        "launch manifest accepted: disk={} uefi_vars={} ram_mib={} vcpus={}",
        manifest.disk(),
        manifest.uefi_vars(),
        manifest.ram_mib(),
        manifest.vcpus()
    );
    Ok(())
}

#[cfg(test)]
#[path = "launch_spec_tests.rs"]
mod tests;
