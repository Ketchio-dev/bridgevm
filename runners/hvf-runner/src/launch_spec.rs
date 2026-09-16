//! `--launch-spec`: the typed product entry point.
//!
//! Reads a versioned JSON manifest (or stdin for `-`), hands it to
//! `bridgevm-hvf-runtime` for validation, and reports what was accepted.
//! This is the replacement for `--launch`, which shells out to a repository
//! script; here nothing is executed through a shell and release builds
//! refuse manifests that point into a source repository.

use anyhow::{bail, Context, Result};
use bridgevm_hvf_runtime::{prepare, LaunchManifest};
use std::io::Read;
use std::path::Path;

/// Release builds enforce product policy; debug builds are evidence
/// harnesses and may run from a repository checkout.
const PRODUCT_POLICY: bool = !cfg!(debug_assertions);

#[path = "launch_spec_owned.rs"]
mod owned;

pub(crate) fn run_launch_spec(spec: &str, args: &crate::Args) -> Result<()> {
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
    println!(
        "launch manifest accepted: disk={} uefi_vars={} ram_mib={} vcpus={}",
        manifest.disk(),
        manifest.uefi_vars(),
        manifest.ram_mib(),
        manifest.vcpus()
    );
    if let Some(helper) = &args.helper {
        return owned::run(manifest, args, helper);
    }
    // Validation-only mode: take the exclusive writer leases now, before any
    // VM could exist: a second writer must fail here, not corrupt the guest
    // at a later flush.
    let prepared = match prepare(manifest, "hvf-runner --launch-spec") {
        Ok(prepared) => prepared,
        Err(error) => bail!("launch refused: {error}"),
    };
    println!(
        "vm prepared: images leased, generation {}",
        prepared.generation().stamp().value()
    );
    Ok(())
}

fn default_receipt_path() -> String {
    std::env::temp_dir()
        .join("bridgevm-reset.receipt")
        .to_string_lossy()
        .into_owned()
}

/// The repo firmware in a checkout; the app bundle passes --helper-firmware.
fn default_firmware_code() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd")
}

#[cfg(test)]
#[path = "launch_spec_tests.rs"]
mod tests;
