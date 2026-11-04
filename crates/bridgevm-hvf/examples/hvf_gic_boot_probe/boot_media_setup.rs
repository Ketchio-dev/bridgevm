use super::*;

pub(crate) fn attach_boot_media(
    platform: &mut VirtPlatform,
    media: &VirtBootMediaConfig,
    platform_cfg: VirtPlatformConfig,
    ram_size: usize,
    vars_data: &[u8],
) {
    // Persisted with the vars at exit, like a choice made in the firmware setup.
    let chosen = std::env::var("BRIDGEVM_GUEST_RESOLUTION").ok();
    let ramfb_desktop = !platform_cfg.devices.virtio_gpu_present;
    let (vars, desktop) = guest_resolution(chosen.as_deref(), vars_data, ramfb_desktop);
    platform.load_flash_vars(vars.as_deref().unwrap_or(vars_data));
    if let Some(mode) = desktop {
        agent_console::request_desktop_mode(mode);
    }
    if let Some(nvme) = media.nvme_disk.as_ref() {
        platform
            .attach_nvme_raw_file(&nvme.path, nvme.write_back)
            .unwrap_or_else(|e| panic!("attach NVMe disk {}: {e}", nvme.path.display()));
        println!(
            "NVMe raw disk attached: {} ({} bytes, write_back={})",
            nvme.path.display(),
            platform.nvme_disk_len(),
            nvme.write_back
        );
        println!(
            "NVMe data path: {}",
            if platform.nvme_direct_dma_enabled() {
                "direct-dma"
            } else {
                "buffered (BRIDGEVM_NVME_BUFFERED_IO=1)"
            }
        );
    }
    if let Some(target) = media.nvme_target.as_ref() {
        platform
            .attach_nvme_second_namespace_raw_file(&target.path, target.write_back)
            .unwrap_or_else(|e| {
                panic!("attach NVMe target (NSID 2) {}: {e}", target.path.display())
            });
        println!(
            "NVMe target namespace (NSID 2) attached: {} (write_back={})",
            target.path.display(),
            target.write_back
        );
    }
    if media.nvme_disk.is_some() || media.nvme_target.is_some() {
        // FLUSH syncs on a worker thread instead of under the platform lock.
        // Checkpoints capture no flush in flight, so they keep the inline sync,
        // which is durable: no worker timer follows an inline barrier.
        let var = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        let checkpoint = var("BRIDGEVM_CHECKPOINT_STATE") || var("BRIDGEVM_RESTORE_STATE");
        let inline = std::env::var("BRIDGEVM_NVME_ASYNC_FLUSH").as_deref() == Ok("0") || checkpoint;
        platform.set_nvme_async_flush(!inline);
        println!(
            "NVMe flush: {}, host sync {}",
            if inline { "inline" } else { "worker thread" },
            bridgevm_hvf::nvme::host_sync_label(inline)
        );
        let unlocked = nvme_unlocked_io(
            std::env::var("BRIDGEVM_NVME_UNLOCKED_IO").ok().as_deref(),
            checkpoint,
        );
        platform.set_nvme_unlocked_io(unlocked);
        println!(
            "NVMe I/O: {}",
            if unlocked {
                "unlocked (reads, writes and TRIM run on per-vCPU workers without the platform lock; a vCPU waits at most 1 ms; BRIDGEVM_NVME_UNLOCKED_IO=0 opts out)"
            } else if checkpoint {
                "under the platform lock (checkpoints keep reads, writes and TRIM inline)"
            } else {
                "under the platform lock (BRIDGEVM_NVME_UNLOCKED_IO=0)"
            }
        );
    }
    let mut pci_installer_iso_attached = false;
    let mut legacy_mmio_installer_iso_attached = false;
    if let Some(path) = media.installer_iso_path.as_ref() {
        match media.installer_iso_transport {
            InstallerIsoTransport::Pci if platform_cfg.devices.virtio_boot_media_present => {
                platform
                    .attach_pci_boot_media(path)
                    .unwrap_or_else(|e| panic!("attach PCI installer ISO {}: {e}", path.display()));
                pci_installer_iso_attached = true;
                println!(
                    "Installer ISO attached on PCI boot media 00:03.0: {}",
                    path.display()
                );
            }
            InstallerIsoTransport::Pci => {
                println!(
                    "Installer ISO PCI boot media disabled; not attaching {}",
                    path.display()
                );
            }
            InstallerIsoTransport::Mmio if platform_cfg.devices.legacy_virtio_mmio_present => {
                platform.attach_virtio_iso(path).unwrap_or_else(|e| {
                    panic!("attach legacy MMIO installer ISO {}: {e}", path.display())
                });
                legacy_mmio_installer_iso_attached = true;
                println!(
                    "Installer ISO attached on legacy virtio-mmio slot {INSTALLER_ISO_SLOT}: {}",
                    path.display()
                );
            }
            InstallerIsoTransport::Mmio => {
                println!(
                    "Installer ISO legacy virtio-mmio disabled; not attaching {}",
                    path.display()
                );
            }
        }
    }
    device_shape::print_device_shape(
        platform_cfg.devices,
        pci_installer_iso_attached,
        legacy_mmio_installer_iso_attached,
        platform.nvme_disk_len(),
    );
    if let Some(linux) = media.linux_boot.as_ref() {
        let kernel = linux
            .read_kernel_bounded(ram_size)
            .unwrap_or_else(|e| panic!("read Linux kernel {}: {e}", linux.kernel_path.display()));
        let initrd = linux
            .read_initrd_bounded(ram_size)
            .unwrap_or_else(|e| panic!("read Linux initrd: {e}"));
        println!(
            "Linux kernel loaded: {} ({} bytes)",
            linux.kernel_path.display(),
            kernel.len()
        );
        if let Some(path) = linux.initrd_path.as_ref() {
            println!(
                "Linux initrd loaded: {} ({} bytes)",
                path.display(),
                initrd.as_ref().map_or(0, Vec::len)
            );
        }
        println!(
            "Linux cmdline loaded: {:?} ({} bytes including NUL)",
            linux.cmdline,
            linux.cmdline_bytes().len()
        );
        platform.set_linux_boot_blobs(kernel, initrd, linux.cmdline_bytes());
    }
}

/// The vars with the firmware's preferred resolution for `chosen`
/// (`BRIDGEVM_GUEST_RESOLUTION`), `None` when they stay as they are, and the
/// mode the agent switches the Windows desktop to once its service starts.
///
/// Windows keeps the desktop mode it saved, whatever the firmware's mode, so
/// the boot that changes the mode switches it (Windows saves the switch). On
/// the ramfb desktop a chosen mode is switched to in every boot: vars that
/// already ask for it do not show that Windows has it (it was set in the
/// firmware setup, or the boot that set it stopped before the switch ran). A
/// virtio-gpu display's desktop follows the window instead. The default is
/// switched to only in the boot that adds it, so a mode set in Windows stays.
fn guest_resolution(
    chosen: Option<&str>,
    vars: &[u8],
    ramfb_desktop: bool,
) -> (Option<Vec<u8>>, Option<(u32, u32)>) {
    use bridgevm_hvf::uefi_resolution as res;
    let report = |mode: (u32, u32), how| {
        println!("UEFI vars: preferred resolution {}x{} {how} (PlatformConfig)", mode.0, mode.1);
    };
    match chosen.map(|text| (text, res::parse_resolution(text))) {
        Some((_, Some(mode))) => {
            let vars = res::with_resolution(vars, mode);
            if vars.is_some() {
                report(mode, "set");
            }
            let switch = vars.is_some() || ramfb_desktop;
            (vars, switch.then_some(mode))
        }
        Some((text, None)) => {
            println!("UEFI vars: guest resolution {text:?} is not a firmware mode; kept");
            (None, None)
        }
        None => {
            let vars = res::with_default_resolution(vars);
            let mode = vars.is_some().then_some(res::DEFAULT_RESOLUTION);
            if let Some(mode) = mode {
                report(mode, "added");
            }
            (vars, mode)
        }
    }
}

/// NVMe reads, writes and TRIM run without the platform lock, on per-vCPU
/// workers that a vCPU waits at most 1 ms for, unless
/// `BRIDGEVM_NVME_UNLOCKED_IO=0`.
/// Under host disk pressure this took away the VM-freeze tail without the late
/// audio service that vCPU-run transfers caused (PR #489, reruns R2 and R3).
/// Checkpoints keep the I/O inline: they capture none in flight.
fn nvme_unlocked_io(setting: Option<&str>, checkpoint: bool) -> bool {
    setting != Some("0") && !checkpoint
}

#[cfg(test)]
#[path = "boot_media_setup_tests.rs"]
mod tests;
