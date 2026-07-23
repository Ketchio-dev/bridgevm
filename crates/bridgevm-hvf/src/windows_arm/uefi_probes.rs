//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UefiFirmwareFileVerification {
    pub(crate) bytes: u64,
    pub(crate) volume: UefiFirmwareVolumeMetadata,
}

pub fn probe_windows_11_arm_uefi_firmware_handoff(
    options: WindowsArmUefiFirmwareHandoffOptions,
) -> WindowsArmUefiFirmwareHandoffProbe {
    let mut blockers = Vec::new();
    let mut firmware_bytes = None;
    let mut firmware_volume = None;
    let mut firmware_verified = false;
    let mut vars_template_bytes = None;
    let mut vars_template_verified = false;
    let mut vars_bytes = None;
    let mut vars_created = false;
    let mut vars_reopened_for_verification = false;
    let mut vars_volume = None;
    let mut vars_verified = false;

    match verify_uefi_firmware_file(&options.firmware_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
        Ok(verification) => {
            firmware_bytes = Some(verification.bytes);
            firmware_volume = Some(verification.volume);
            firmware_verified = true;
        }
        Err(error) => blockers.push(format!("firmware verification failed: {error}")),
    }

    if let Some(template_path) = &options.vars_template_path {
        match verify_uefi_firmware_file(template_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
            Ok(verification) => {
                vars_template_bytes = Some(verification.bytes);
                vars_template_verified = true;
            }
            Err(error) => blockers.push(format!("vars template verification failed: {error}")),
        }
    }

    if options.create_vars {
        match (&options.vars_template_path, &options.vars_path) {
            (Some(template_path), Some(vars_path)) => {
                if vars_path.exists() {
                    blockers.push(format!(
                        "vars path already exists; refusing to overwrite {}",
                        vars_path.display()
                    ));
                } else if vars_template_verified {
                    match copy_uefi_vars_template(template_path, vars_path) {
                        Ok(()) => {
                            vars_created = true;
                            match verify_uefi_firmware_file(vars_path, WINDOWS_ARM_UEFI_SLOT_BYTES)
                            {
                                Ok(verification) => {
                                    vars_bytes = Some(verification.bytes);
                                    vars_volume = Some(verification.volume);
                                    vars_reopened_for_verification = true;
                                    vars_verified = true;
                                }
                                Err(error) => blockers.push(format!(
                                    "created vars store verification failed: {error}"
                                )),
                            }
                        }
                        Err(error) => blockers.push(format!("vars creation failed: {error}")),
                    }
                }
            }
            (None, _) => blockers.push(
                "--vars-template is required with --create-vars for a mutable UEFI variable store"
                    .to_string(),
            ),
            (_, None) => blockers.push(
                "--vars is required with --create-vars for a mutable UEFI variable store"
                    .to_string(),
            ),
        }
    } else if let Some(vars_path) = &options.vars_path {
        match verify_uefi_firmware_file(vars_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
            Ok(verification) => {
                vars_bytes = Some(verification.bytes);
                vars_volume = Some(verification.volume);
                vars_reopened_for_verification = true;
                vars_verified = true;
            }
            Err(error) => blockers.push(format!("vars store verification failed: {error}")),
        }
    } else if options.vars_template_path.is_some() {
        blockers.push(
            "vars template was verified, but no mutable --vars path was supplied".to_string(),
        );
    } else {
        blockers.push("UEFI variable store is required for Windows firmware handoff".to_string());
    }

    let planned_reset_vector_ipa =
        (firmware_verified && vars_verified).then_some(WINDOWS_ARM_UEFI_CODE_IPA);

    WindowsArmUefiFirmwareHandoffProbe {
        firmware_path: options.firmware_path,
        firmware_bytes,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_volume,
        firmware_verified,
        vars_template_path: options.vars_template_path,
        vars_template_bytes,
        vars_template_verified,
        vars_path: options.vars_path,
        vars_bytes,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        vars_slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        vars_created,
        vars_reopened_for_verification,
        vars_volume,
        vars_verified,
        planned_reset_vector_ipa,
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_map(
    options: WindowsArmUefiPflashMapOptions,
) -> WindowsArmUefiPflashMapProbe {
    let handoff =
        probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
            firmware_path: options.firmware_path,
            vars_template_path: options.vars_template_path,
            vars_path: options.vars_path,
            create_vars: options.create_vars,
        });
    let mut blockers = handoff.blockers.clone();

    let firmware_slot = if handoff.firmware_verified {
        match load_uefi_pflash_slot(
            "code",
            &handoff.firmware_path,
            WINDOWS_ARM_UEFI_CODE_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
            false,
        ) {
            Ok(slot) => Some(slot),
            Err(error) => {
                blockers.push(format!("firmware pflash load failed: {error}"));
                None
            }
        }
    } else {
        None
    };

    let vars_slot = if handoff.vars_verified {
        match &handoff.vars_path {
            Some(vars_path) => match load_uefi_pflash_slot(
                "vars",
                vars_path,
                WINDOWS_ARM_UEFI_VARS_IPA,
                WINDOWS_ARM_UEFI_SLOT_BYTES,
                true,
            ) {
                Ok(slot) => Some(slot),
                Err(error) => {
                    blockers.push(format!("vars pflash load failed: {error}"));
                    None
                }
            },
            None => {
                blockers.push("verified vars store has no path for pflash mapping".to_string());
                None
            }
        }
    } else {
        None
    };

    let firmware_slot_loaded = firmware_slot
        .as_ref()
        .is_some_and(|slot| slot.prefix_verified && slot.padding_zeroed);
    let vars_slot_loaded = vars_slot
        .as_ref()
        .is_some_and(|slot| slot.prefix_verified && slot.padding_zeroed);

    let pflash_slots_non_overlapping = match (&firmware_slot, &vars_slot) {
        (Some(firmware_slot), Some(vars_slot)) => {
            firmware_slot.ipa_start == WINDOWS_ARM_UEFI_CODE_IPA
                && firmware_slot.ipa_end_exclusive() == WINDOWS_ARM_UEFI_VARS_IPA
                && vars_slot.ipa_start == WINDOWS_ARM_UEFI_VARS_IPA
                && vars_slot.ipa_end_exclusive() == WINDOWS_ARM_DEVICE_MMIO_IPA
                && !ipa_ranges_overlap(
                    firmware_slot.ipa_start,
                    firmware_slot.slot_bytes,
                    vars_slot.ipa_start,
                    vars_slot.slot_bytes,
                )
        }
        _ => false,
    };
    let guest_ram_overlap_verified = [&firmware_slot, &vars_slot]
        .into_iter()
        .flatten()
        .all(|slot| slot.ipa_end_exclusive() <= WINDOWS_ARM_GUEST_RAM_IPA);
    let device_mmio_overlap_verified =
        [&firmware_slot, &vars_slot]
            .into_iter()
            .flatten()
            .all(|slot| {
                !ipa_ranges_overlap(
                    slot.ipa_start,
                    slot.slot_bytes,
                    WINDOWS_ARM_DEVICE_MMIO_IPA,
                    WINDOWS_ARM_DEVICE_MMIO_BYTES,
                )
            });
    let pflash_map_verified = firmware_slot_loaded
        && vars_slot_loaded
        && pflash_slots_non_overlapping
        && guest_ram_overlap_verified
        && device_mmio_overlap_verified;

    if (firmware_slot_loaded || vars_slot_loaded) && !pflash_slots_non_overlapping {
        blockers.push("pflash code/vars IPA range verification failed".to_string());
    }
    if (firmware_slot_loaded || vars_slot_loaded) && !guest_ram_overlap_verified {
        blockers.push("pflash slots overlap the planned guest RAM window".to_string());
    }
    if (firmware_slot_loaded || vars_slot_loaded) && !device_mmio_overlap_verified {
        blockers.push("pflash slots overlap the planned device MMIO window".to_string());
    }

    WindowsArmUefiPflashMapProbe {
        firmware_path: handoff.firmware_path,
        vars_path: handoff.vars_path,
        vars_created: handoff.vars_created,
        firmware_verified: handoff.firmware_verified,
        vars_verified: handoff.vars_verified,
        firmware_slot,
        vars_slot,
        pflash_region_start: WINDOWS_ARM_UEFI_CODE_IPA,
        pflash_region_bytes: WINDOWS_ARM_UEFI_PFLASH_BYTES,
        pflash_slots_non_overlapping,
        guest_ram_overlap_verified,
        device_mmio_overlap_verified,
        pflash_map_verified,
        planned_reset_vector_ipa: pflash_map_verified.then_some(WINDOWS_ARM_UEFI_CODE_IPA),
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_hvf_map(
    options: WindowsArmUefiPflashMapOptions,
    allow_map: bool,
) -> WindowsArmUefiPflashHvfMapProbe {
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(options);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_pflash_hvf_map(allow_map, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_reset_vector_entry(
    options: WindowsArmUefiPflashMapOptions,
    allow_entry: bool,
) -> WindowsArmUefiResetVectorEntryProbe {
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(options);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_reset_vector_entry(allow_entry, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_firmware_run_loop(
    options: WindowsArmUefiFirmwareRunLoopOptions,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let WindowsArmUefiFirmwareRunLoopOptions { pflash, execution } = options;
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(pflash);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_firmware_run_loop(execution, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_firmware_device_discovery(
    options: WindowsArmUefiFirmwareRunLoopOptions,
) -> WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    let mut options = options;
    options.execution.map_low_pflash_alias = true;
    options.execution.repair_low_vector_diagnostic_page = true;
    options.execution.continue_after_low_vector_repair = true;
    options.execution.wire_interrupt_timer = true;
    options.execution.stop_at_first_post_repair_device_boundary = true;
    WindowsArmUefiFirmwareDeviceDiscoveryProbe {
        run_loop: probe_windows_11_arm_uefi_firmware_run_loop(options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uefi_fv_bytes(len: usize) -> Vec<u8> {
        assert!(len >= UEFI_FV_MIN_HEADER_BYTES);
        let header_length = 0x48_u16;
        let mut bytes = vec![0_u8; len];
        bytes[16..32].copy_from_slice(&[
            0x8c, 0x8c, 0xf9, 0x61, 0xd2, 0x4b, 0x2c, 0x4f, 0x8a, 0x89, 0x22, 0x4d, 0xaf, 0xdc,
            0xf1, 0x6f,
        ]);
        bytes[UEFI_FV_LENGTH_OFFSET..UEFI_FV_LENGTH_OFFSET + 8]
            .copy_from_slice(&(len as u64).to_le_bytes());
        bytes[UEFI_FV_SIGNATURE_OFFSET..UEFI_FV_SIGNATURE_OFFSET + 4]
            .copy_from_slice(UEFI_FV_SIGNATURE);
        bytes[0x2c..0x30].copy_from_slice(&0x0004_feff_u32.to_le_bytes());
        bytes[UEFI_FV_HEADER_LENGTH_OFFSET..UEFI_FV_HEADER_LENGTH_OFFSET + 2]
            .copy_from_slice(&header_length.to_le_bytes());
        bytes[0x34..0x36].copy_from_slice(&0_u16.to_le_bytes());
        bytes[0x36] = 0;
        bytes[0x37] = 2;
        bytes[0x38..0x3c].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x3c..0x40].copy_from_slice(&(len as u32).to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(&0_u32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&0_u32.to_le_bytes());
        let checksum = 0_u16.wrapping_sub(uefi_checksum16(&bytes[..usize::from(header_length)]));
        bytes[0x32..0x34].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn windows_11_arm_uefi_firmware_handoff_probe_requires_vars_store() {
        let firmware_path = std::env::temp_dir().join(format!(
            "bridgevm-windows-arm-uefi-handoff-missing-vars-{}-{}.fd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(64 * 1024)).unwrap();

        let probe =
            probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
                firmware_path: firmware_path.clone(),
                vars_template_path: None,
                vars_path: None,
                create_vars: false,
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);

        assert!(probe.firmware_verified);
        assert!(!probe.vars_verified);
        assert_eq!(probe.planned_reset_vector_ipa, None);
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("UEFI variable store is required")));
        assert!(output.contains("Firmware verified: true"));
        assert!(output.contains("Vars verified: false"));
        assert!(output.contains("UEFI variable store is required"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
