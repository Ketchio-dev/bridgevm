use super::*;

pub(crate) unsafe fn prepare_platform(
    media: &VirtBootMediaConfig,
    platform_cfg: VirtPlatformConfig,
    tpm_backend: Option<Box<dyn Tpm2Backend>>,
) -> (VirtPlatform, Vec<u8>, Vec<u8>) {
    map_file(
        &media.firmware_code_path,
        machine::FLASH_CODE.base,
        machine::FLASH_CODE.size as usize,
        HV_MEMORY_READ | HV_MEMORY_EXEC,
    );
    let vars_data = media
        .flash_vars
        .read_bounded(machine::FLASH_VARS.size as usize)
        .unwrap_or_else(|e| panic!("read UEFI vars {}: {e}", media.flash_vars.path.display()));
    let mut platform = VirtPlatform::new_with_config_and_tpm_backend(platform_cfg, tpm_backend);
    if env_flag("BRIDGEVM_HDA_COREAUDIO") {
        if !platform_cfg.devices.hda_present {
            eprintln!("BRIDGEVM_HDA_COREAUDIO ignored because BRIDGEVM_HDA is not enabled");
        } else {
            // A Mac output that cannot start turns sound off; the VM still runs.
            #[cfg(target_os = "macos")]
            match hda_coreaudio::start_output(hda_coreaudio::CoreAudioPcmSink::new, |sink| {
                sink.install(&mut platform)
            }) {
                Ok(()) => println!("{}", hda_coreaudio::ENABLED),
                Err(line) => eprintln!("{line}"),
            }
            #[cfg(not(target_os = "macos"))]
            panic!("BRIDGEVM_HDA_COREAUDIO is only supported on macOS");
        }
    }
    hda_capture_setup::install_hda_capture(&mut platform, platform_cfg.devices.hda_present);
    if platform_cfg.devices.ramfb_present {
        println!("ramfb fw_cfg: enabled");
    } else if env_flag("BRIDGEVM_RAMFB") {
        println!("ramfb fw_cfg: disabled by BRIDGEVM_DISABLE_RAMFB_DEVICE");
    } else {
        println!("ramfb fw_cfg: disabled");
    }
    if !platform_cfg.devices.xhci_present {
        println!("qemu-xhci: disabled by BRIDGEVM_DISABLE_XHCI");
    }
    if !platform_cfg.devices.virtio_boot_media_present {
        println!("virtio installer ISO surfaces: disabled by BRIDGEVM_DISABLE_VIRTIO_ISO");
    }
    let xhci_report_interval = parse_xhci_report_interval_env();
    platform.set_xhci_report_interval(xhci_report_interval);
    // Only keystrokes need the wider gap; pointer reports follow host events.
    let pointer_interval = xhci_report_interval.min(XHCI_POINTER_REPORT_INTERVAL);
    platform.set_xhci_pointer_report_interval(pointer_interval);
    if xhci_report_interval.is_zero() {
        println!("xHCI HID report pacing: disabled (BRIDGEVM_XHCI_REPORT_INTERVAL_MS=0)");
    } else {
        println!(
            "xHCI HID report pacing: {} ms between reports ({} ms for pointer reports)",
            xhci_report_interval.as_millis(),
            pointer_interval.as_millis()
        );
    }

    let boot_dtb = platform.dtb().to_vec();
    (platform, vars_data, boot_dtb)
}
