use super::*;

pub(crate) unsafe fn prepare_platform(
    media: &VirtBootMediaConfig,
    platform_cfg: VirtPlatformConfig,
    swtpm_data_socket: Option<&Path>,
    swtpm_control_socket: Option<&Path>,
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
    let tpm_backend: Option<Box<dyn Tpm2Backend>> =
        swtpm_data_socket
            .as_ref()
            .map(|path| match swtpm_control_socket.as_ref() {
                Some(control) => {
                    println!(
                        "TPM2 TIS backend: swtpm data socket {} control socket {}",
                        path.display(),
                        control.display()
                    );
                    Box::new(
                        SwtpmUnixBackend::connect_with_control(path, Some(control)).unwrap_or_else(
                            |error| {
                                panic!(
                                    "connect swtpm data {} control {}: {error}",
                                    path.display(),
                                    control.display()
                                )
                            },
                        ),
                    ) as Box<dyn Tpm2Backend>
                }
                None => {
                    println!("TPM2 TIS backend: swtpm data socket {}", path.display());
                    Box::new(SwtpmUnixBackend::connect(path).unwrap_or_else(|error| {
                        panic!("connect swtpm {}: {error}", path.display())
                    })) as Box<dyn Tpm2Backend>
                }
            });
    let mut platform = VirtPlatform::new_with_config_and_tpm_backend(platform_cfg, tpm_backend);
    if env_flag("BRIDGEVM_HDA_COREAUDIO") {
        if !platform_cfg.devices.hda_present {
            eprintln!("BRIDGEVM_HDA_COREAUDIO ignored because BRIDGEVM_HDA is not enabled");
        } else {
            #[cfg(target_os = "macos")]
            {
                let sink = hda_coreaudio::CoreAudioPcmSink::new()
                    .unwrap_or_else(|error| panic!("initialize HDA CoreAudio output: {error}"));
                platform.set_hda_pcm_sink(Some(Box::new(sink)));
                println!("HDA CoreAudio output: s16le 48000 Hz stereo, enabled");
            }
            #[cfg(not(target_os = "macos"))]
            panic!("BRIDGEVM_HDA_COREAUDIO is only supported on macOS");
        }
    }
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
    if xhci_report_interval.is_zero() {
        println!("xHCI HID report pacing: disabled (BRIDGEVM_XHCI_REPORT_INTERVAL_MS=0)");
    } else {
        println!(
            "xHCI HID report pacing: {} ms between reports",
            xhci_report_interval.as_millis()
        );
    }

    let boot_dtb = platform.dtb().to_vec();
    (platform, vars_data, boot_dtb)
}
