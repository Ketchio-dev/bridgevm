use super::*;

pub(crate) fn attach_boot_media(
    platform: &mut VirtPlatform,
    media: &VirtBootMediaConfig,
    platform_cfg: VirtPlatformConfig,
    ram_size: usize,
    vars_data: &[u8],
) {
    // Persisted with the vars at exit, like a choice made in the firmware setup.
    use bridgevm_hvf::uefi_resolution as res;
    let chosen = std::env::var("BRIDGEVM_GUEST_RESOLUTION").ok();
    let update = match chosen.as_deref().map(|text| (text, res::parse_resolution(text))) {
        Some((_, Some(mode))) => res::with_resolution(vars_data, mode).map(|v| (v, mode, "set")),
        Some((text, None)) => {
            println!("UEFI vars: guest resolution {text:?} is not a firmware mode; kept");
            None
        }
        None => res::with_default_resolution(vars_data).map(|v| (v, res::DEFAULT_RESOLUTION, "added")),
    };
    // Windows keeps its saved desktop mode: switch it in the boot that changed
    // the firmware's mode (Windows saves the switch).
    match update {
        Some((vars, mode, how)) => {
            println!("UEFI vars: preferred resolution {}x{} {how} (PlatformConfig)", mode.0, mode.1);
            platform.load_flash_vars(&vars);
            agent_console::request_desktop_mode(mode);
        }
        None => platform.load_flash_vars(vars_data),
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
        // Checkpoints capture no flush in flight, so they keep the inline sync.
        let var = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        let inline = std::env::var("BRIDGEVM_NVME_ASYNC_FLUSH").as_deref() == Ok("0")
            || var("BRIDGEVM_CHECKPOINT_STATE")
            || var("BRIDGEVM_RESTORE_STATE");
        platform.set_nvme_async_flush(!inline);
        println!(
            "NVMe flush: {}, host sync {}",
            if inline { "inline" } else { "worker thread" },
            bridgevm_hvf::nvme::host_sync_label()
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
