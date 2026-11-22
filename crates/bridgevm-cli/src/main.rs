//! The `bridgevm` command-line front-end.
//!
//! Forwards native app commands to the packaged app CLI and runs the local
//! own-HVF tools and report commands.

use anyhow::{bail, Context, Result};
use bridgevm_hvf::{
    plan_windows_11_arm_hvf_machine, plan_windows_11_arm_no_qemu, probe_hvf_guest_entry,
    probe_hvf_guest_exit_loop, probe_hvf_interrupt_timer, probe_hvf_memory_map,
    probe_hvf_mmio_block_device, probe_hvf_mmio_block_queue, probe_hvf_mmio_read_emulation,
    probe_hvf_mmio_read_exit, probe_hvf_mmio_rtc_device, probe_hvf_mmio_serial_device,
    probe_hvf_mmio_write_emulation, probe_hvf_vcpu_create, probe_hvf_vcpu_run, probe_hvf_vm_create,
    probe_hvf_vtimer_exit, probe_virtio_block_file_backing, probe_virtio_block_iso_backing,
    probe_virtio_block_request_model, probe_virtio_block_writable_file_backing,
    probe_virtio_gpu_3d_host_preflight_for, probe_windows_11_arm_boot_disk_layout,
    probe_windows_11_arm_platform_description, probe_windows_11_arm_uefi_firmware_device_discovery,
    probe_windows_11_arm_uefi_firmware_handoff, probe_windows_11_arm_uefi_firmware_run_loop,
    probe_windows_11_arm_uefi_pflash_hvf_map, probe_windows_11_arm_uefi_pflash_map,
    probe_windows_11_arm_uefi_reset_vector_entry, probe_windows_11_arm_xhci_hid_boot_key_report,
    query_hvf_host_capabilities, HvfMachinePlanOptions, VirtioGpu3dHostPreflightProtocol,
    WindowsArmBootDiskLayoutOptions, WindowsArmPlatformDescriptionOptions,
    WindowsArmUefiFirmwareHandoffOptions, WindowsArmUefiFirmwareRunLoopExecutionOptions,
    WindowsArmUefiFirmwareRunLoopOptions, WindowsArmUefiPflashMapOptions,
    WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB,
};
use clap::{Parser, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

#[cfg(test)]
mod test_support;

mod app_cli;
mod app_cli_resolver;
mod args;
mod entry;
mod hvf_cmd;
mod json_util;
mod report;
mod title_gate;

use args::*;
use hvf_cmd::*;
use json_util::*;
use report::*;
use title_gate::*;

fn main() -> anyhow::Result<()> {
    entry::run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_hvf_plan_render_is_blocked_and_qemu_free() {
        let plan =
            plan_windows_11_arm_no_qemu(Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")));
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm no-QEMU HVF plan"));
        assert!(output.contains("Engine: BridgeVM HVF"));
        assert!(output.contains("Substrate: Apple Hypervisor.framework"));
        assert!(output.contains("Installer: ISO/Win11_25H2_English_Arm64_v2.iso"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
