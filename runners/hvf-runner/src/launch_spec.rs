//! `--launch-spec`: the typed product entry point.
//!
//! Reads a versioned JSON manifest (or stdin for `-`), hands it to
//! `bridgevm-hvf-runtime` for validation, and reports what was accepted.
//! This is the replacement for `--launch`, which shells out to a repository
//! script; here nothing is executed through a shell and release builds
//! refuse manifests that point into a source repository.

use anyhow::{bail, Context, Result};
use bridgevm_hvf_runtime::{
    prepare, run_vm, start_swtpm, DeviceSurfaces, HelperLaunch, LaunchManifest, ShareSurface,
    VtpmConfig,
};
use std::io::Read;
use std::path::Path;

/// Release builds enforce product policy; debug builds are evidence
/// harnesses and may run from a repository checkout.
const PRODUCT_POLICY: bool = !cfg!(debug_assertions);

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
        // The full composed lifecycle: leases, helper generations under the
        // reset-cycle supervisor, flush + receipt between generations.
        let mut launch = HelperLaunch {
            helper: helper.clone(),
            firmware_code: args
                .helper_firmware
                .clone()
                .unwrap_or_else(default_firmware_code),
            watchdog_ms: args.watchdog_ms.unwrap_or(900_000),
            agent_control: args.helper_agent_control.clone(),
            surfaces: args.helper_evidence_dir.as_ref().map(|dir| DeviceSurfaces {
                evidence_dir: dir.clone(),
                display_export_ms: 100,
                input_control: Some(dir.join("input.ctl")),
                virtio_gpu_3d: args.virtio_gpu_3d,
                clipboard_sync: args.agent_clipboard_sync,
                share: args
                    .agent_share_host
                    .as_ref()
                    .zip(args.agent_share_guest.as_ref())
                    .map(|(host, guest)| ShareSurface {
                        host_dir: host.clone(),
                        guest_dir: guest.clone(),
                        interval_ms: args.agent_share_ms.unwrap_or(2000),
                        max_kb: args.agent_share_max_kb.unwrap_or(65536),
                    }),
                virtio_net: args.virtio_net,
                hda_audio: false,
            }),
            swtpm_sockets: None,
        };
        let _swtpm = match &args.helper_vtpm_state {
            Some(state_dir) => {
                let process = start_swtpm(&VtpmConfig {
                    state_dir: state_dir.clone(),
                    swtpm_bin: "/opt/homebrew/bin/swtpm".into(),
                    state_key: None,
                })
                .map_err(|error| anyhow::anyhow!("vTPM start failed: {error}"))?;
                launch.swtpm_sockets = Some((
                    process.data_socket().to_path_buf(),
                    process.control_socket().to_path_buf(),
                ));
                println!("vtpm: swtpm serving {}", process.data_socket().display());
                Some(process)
            }
            None => None,
        };
        if let Some(surfaces) = &launch.surfaces {
            std::fs::create_dir_all(surfaces.evidence_dir.join("ramfb"))
                .context("create evidence dir")?;
        }
        let receipt = args
            .supervise_receipt
            .clone()
            .unwrap_or_else(default_receipt_path);
        let cycles = run_vm(
            manifest,
            &launch,
            Path::new(&receipt),
            args.supervise_max_cycles,
            "hvf-runner --launch-spec",
        )
        .map_err(|error| anyhow::anyhow!("vm run failed: {error}"))?;
        for cycle in &cycles {
            println!(
                "vm cycle: generation={} helper_pid={}",
                cycle.generation, cycle.pid
            );
        }
        println!("vm run complete: {} generation(s)", cycles.len());
        return Ok(());
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
