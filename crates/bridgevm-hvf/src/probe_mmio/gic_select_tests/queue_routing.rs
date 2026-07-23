//! virtio-block queue notify/backing selection and register routing.

use super::helpers::*;
use crate::probe_mmio::*;
use crate::*;

#[test]
fn firmware_block_queue_notify_selects_backing_by_mmio_ipa() {
    let stem = format!(
        "bridgevm-hvf-firmware-block-queue-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let iso_path = std::env::temp_dir().join(format!("{stem}.iso"));
    let disk_path = std::env::temp_dir().join(format!("{stem}.raw"));
    let sector_start = (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
    let mut iso = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
    let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 32];
    for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
        iso[sector_start + offset] = 0xc0_u8.wrapping_add(offset as u8);
        disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
    }
    std::fs::write(&iso_path, &iso).unwrap();
    std::fs::write(&disk_path, &disk).unwrap();

    let block_devices =
        windows_arm_firmware_block_devices(Some(iso_path.clone()), Some(disk_path.clone()));
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    assert_eq!(block_devices[0].capacity_sectors, 16);
    assert_eq!(block_devices[1].capacity_sectors, 32);
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
            4,
        )),
        MmioAction::ReadValue(16)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
            4,
        )),
        MmioAction::ReadValue(32)
    );

    let installer_notify_ipa =
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
    let mut installer_backing = vec![0_u8; 16 * 1024];
    let mut installer_memory =
        VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut installer_backing);
    configure_virtio_block_queue_on_bus(&mut bus, WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA);
    seed_synthetic_virtio_block_read_request(&mut installer_memory).unwrap();
    let installer_status_ipa =
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET;
    let gicd_spi_pending_clear_ipa =
        WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ICPENDR_BASE_OFFSET + 4;
    let gicd_spi_pending_set_ipa =
        WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4;
    assert!(windows_arm_firmware_block_irq_source_may_change(
        &block_devices,
        installer_status_ipa,
        0,
    ));
    assert!(!windows_arm_firmware_block_irq_source_may_change(
        &block_devices,
        installer_status_ipa,
        VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE,
    ));
    assert!(
        windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
            gicd_spi_pending_clear_ipa,
            0x4,
            4,
        )
    );
    assert!(
        !windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
            gicd_spi_pending_set_ipa,
            0x4,
            4,
        )
    );
    assert!(
        !windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
            gicd_spi_pending_clear_ipa,
            0,
            4,
        )
    );
    assert_eq!(
        complete_windows_arm_firmware_block_queue_notify(
            &mut bus,
            &mut installer_memory,
            &block_devices,
            installer_status_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        ),
        Err(VirtioBlockRequestError::UnexpectedQueueNotifyIpa {
            role: "installer-iso",
            ipa: installer_status_ipa,
        })
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(installer_notify_ipa, 1, 4)),
        MmioAction::WriteAccepted { value: 1, byte: 1 }
    );
    assert_eq!(
        complete_windows_arm_firmware_block_queue_notify(
            &mut bus,
            &mut installer_memory,
            &block_devices,
            installer_notify_ipa,
            1,
        ),
        Err(VirtioBlockRequestError::UnsupportedQueueNotifyValue {
            role: "installer-iso",
            value: 1,
        })
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            installer_notify_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            4,
        )),
        MmioAction::WriteAccepted {
            value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
        }
    );
    let installer_completion = complete_windows_arm_firmware_block_queue_notify(
        &mut bus,
        &mut installer_memory,
        &block_devices,
        installer_notify_ipa,
        VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
    )
    .unwrap();
    assert_eq!(installer_completion.role, "installer-iso");
    assert_eq!(installer_completion.backing_kind, "host-iso-readonly");
    assert_eq!(
        installer_completion.base_ipa,
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
    );
    assert_eq!(
        installer_completion.completion.request_type,
        VIRTIO_BLK_T_IN
    );
    assert_eq!(installer_completion.completion.status, VIRTIO_BLK_S_OK);
    assert_eq!(installer_completion.byte_offset, 0xe00);
    assert_eq!(installer_completion.used_len, 513);
    assert_eq!(
        installer_memory
            .read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)
            .unwrap(),
        vec![0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7]
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            4,
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(!refresh_windows_arm_firmware_device_irq_pending(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
            4,
        )),
        MmioAction::ReadValue(0x4)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_CTLR_OFFSET,
            u64::from(GICD_CTLR_ENABLE_GRP1NS),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
            byte: GICD_CTLR_ENABLE_GRP1NS as u8,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IGROUPR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISENABLER_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert!(windows_arm_firmware_device_irq_line_assertable(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::write(gicd_spi_pending_clear_ipa, 0x4, 4)),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert!(!windows_arm_firmware_device_irq_line_assertable(
        &mut bus,
        &block_devices
    ));
    assert!(refresh_windows_arm_firmware_device_irq_pending(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicd_spi_pending_set_ipa, 4)),
        MmioAction::ReadValue(0x4)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET,
            VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
            4,
        )),
        MmioAction::WriteAccepted {
            value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
            byte: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE as u8,
        }
    );
    assert!(!refresh_windows_arm_firmware_device_irq_pending(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
            4,
        )),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert!(windows_arm_firmware_device_irq_line_assertable(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ICPENDR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert!(!windows_arm_firmware_device_irq_line_assertable(
        &mut bus,
        &block_devices
    ));

    let target_notify_ipa =
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
    let mut target_backing = vec![0_u8; 16 * 1024];
    let mut target_memory = VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut target_backing);
    configure_virtio_block_queue_on_bus(&mut bus, WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA);
    seed_synthetic_virtio_block_write_request_as_first(&mut target_memory).unwrap();
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            target_notify_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            4,
        )),
        MmioAction::WriteAccepted {
            value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
        }
    );
    let target_completion = complete_windows_arm_firmware_block_queue_notify(
        &mut bus,
        &mut target_memory,
        &block_devices,
        target_notify_ipa,
        VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
    )
    .unwrap();
    assert_eq!(target_completion.role, "target-disk");
    assert_eq!(target_completion.backing_kind, "host-file-writable");
    assert_eq!(
        target_completion.base_ipa,
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
    );
    assert_eq!(target_completion.completion.request_type, VIRTIO_BLK_T_OUT);
    assert_eq!(target_completion.completion.status, VIRTIO_BLK_S_OK);
    assert_eq!(target_completion.byte_offset, 0xe00);
    assert_eq!(target_completion.used_len, VIRTIO_BLOCK_STATUS_BYTES);
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            4,
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(!refresh_windows_arm_firmware_device_irq_pending(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
            4,
        )),
        MmioAction::ReadValue(0x8)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IGROUPR_BASE_OFFSET + 4,
            0xc,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0xc,
            byte: 0xc,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISENABLER_BASE_OFFSET + 4,
            0x8,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x8,
            byte: 0x8,
        }
    );
    assert!(windows_arm_firmware_device_irq_line_assertable(
        &mut bus,
        &block_devices
    ));
    assert!(refresh_windows_arm_firmware_device_irq_pending(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET,
            VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
            4,
        )),
        MmioAction::WriteAccepted {
            value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
            byte: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE as u8,
        }
    );
    assert!(!refresh_windows_arm_firmware_device_irq_pending(
        &mut bus,
        &block_devices
    ));
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
            4,
        )),
        MmioAction::ReadValue(0)
    );
    let persisted = std::fs::read(&disk_path).unwrap();
    assert_eq!(
        &persisted[sector_start..sector_start + 8],
        &[0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
    );

    let _ = std::fs::remove_file(&iso_path);
    let _ = std::fs::remove_file(&disk_path);
}

#[test]
fn mmio_bus_routes_virtio_block_identity_registers_after_boot_devices() {
    let mut bus = MmioBus::default();
    let block_base = 0x5000_2000;
    let block = VirtioMmioBlockDevice::new(0x5000_2000);
    let magic_ipa = block_base + VIRTIO_MMIO_MAGIC_VALUE_OFFSET;
    let version_ipa = block_base + VIRTIO_MMIO_VERSION_OFFSET;
    let device_id_ipa = block_base + VIRTIO_MMIO_DEVICE_ID_OFFSET;
    let vendor_id_ipa = block_base + VIRTIO_MMIO_VENDOR_ID_OFFSET;

    bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));
    bus.attach(Box::new(Pl031RtcDevice::new(0x5000_1000, 0x2026_0618)));
    bus.attach(Box::new(block));

    assert_eq!(bus.device_count(), 3);
    assert_eq!(
        bus.dispatch(MmioAccess::read(magic_ipa, 4)),
        MmioAction::ReadValue(0x7472_6976)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(version_ipa, 4)),
        MmioAction::ReadValue(2)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(device_id_ipa, 4)),
        MmioAction::ReadValue(2)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(vendor_id_ipa, 4)),
        MmioAction::ReadValue(0x4252_564d)
    );
}

#[test]
fn mmio_bus_routes_virtio_block_queue_and_config_registers() {
    let mut bus = MmioBus::default();
    let block_base = 0x5000_2000;
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));

    let writes = [
        (
            VIRTIO_MMIO_DRIVER_FEATURES_OFFSET,
            VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
        ),
        (
            VIRTIO_MMIO_STATUS_OFFSET,
            VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE,
        ),
        (
            VIRTIO_MMIO_STATUS_OFFSET,
            VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE,
        ),
        (
            VIRTIO_MMIO_STATUS_OFFSET,
            VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE,
        ),
        (
            VIRTIO_MMIO_QUEUE_SEL_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
        ),
        (
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
        (VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_BLOCK_STATUS_VALUE),
        (
            VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        ),
    ];

    assert_eq!(
        bus.dispatch(MmioAccess::read(
            block_base + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
            4
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE)
    );

    for (offset, value) in writes {
        assert_eq!(
            bus.dispatch(MmioAccess::write(block_base + offset, value, 4)),
            MmioAction::WriteAccepted {
                value,
                byte: (value & 0xff) as u8,
            }
        );
    }

    let reads = [
        (
            VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
        ),
        (
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
        (
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
        ),
        (VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_BLOCK_STATUS_VALUE),
        (
            VIRTIO_MMIO_CONFIG_GENERATION_OFFSET,
            VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE,
        ),
        (
            VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32,
        ),
    ];

    for (offset, expected) in reads {
        assert_eq!(
            bus.dispatch(MmioAccess::read(block_base + offset, 4)),
            MmioAction::ReadValue(expected)
        );
    }
}

#[test]
fn virtio_block_status_zero_resets_queue_state() {
    let mut bus = MmioBus::default();
    let block_base = 0x5000_2000;
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut backing);

    configure_virtio_block_queue_on_bus(&mut bus, block_base);
    seed_synthetic_virtio_block_read_request(&mut memory).unwrap();
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            block_base + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            4,
        )),
        MmioAction::WriteAccepted {
            value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
        }
    );
    {
        let block = bus
            .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
            .unwrap();
        block
            .complete_next_available_block_request(&mut memory)
            .unwrap();
    }
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            block_base + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            4,
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            block_base + VIRTIO_MMIO_STATUS_OFFSET,
            0,
            4,
        )),
        MmioAction::WriteAccepted { value: 0, byte: 0 }
    );
    for (offset, expected) in [
        (VIRTIO_MMIO_STATUS_OFFSET, 0),
        (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, 0),
        (VIRTIO_MMIO_QUEUE_NUM_OFFSET, 0),
        (VIRTIO_MMIO_QUEUE_READY_OFFSET, 0),
        (VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, 0),
        (VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, 0),
        (VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, 0),
        (
            VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
        ),
        (
            VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff,
        ),
    ] {
        assert_eq!(
            bus.dispatch(MmioAccess::read(block_base + offset, 4)),
            MmioAction::ReadValue(expected)
        );
    }

    configure_virtio_block_queue_on_bus(&mut bus, block_base);
    seed_synthetic_virtio_block_read_request(&mut memory).unwrap();
    let block = bus
        .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
        .unwrap();
    assert!(block
        .complete_next_available_block_request(&mut memory)
        .is_ok());
}

#[test]
fn virtio_block_completes_one_available_read_request() {
    let block_base = 0x5000_2000;
    let guest_base = 0x4000_0000;
    let header_ipa = guest_base + 0x80;
    let data_ipa = guest_base + 0x400;
    let status_ipa = guest_base + 0x700;
    let sector = 7;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let mut block = VirtioMmioBlockDevice::new(block_base);

    for (offset, value) in [
        (
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        assert!(matches!(
            block.handle(MmioAccess::write(block_base + offset, value, 4)),
            MmioAction::WriteAccepted { .. }
        ));
    }

    memory.write_u32(header_ipa, VIRTIO_BLK_T_IN).unwrap();
    memory.write_u32(header_ipa + 4, 0).unwrap();
    memory.write_u64(header_ipa + 8, sector).unwrap();
    VirtqDescriptor {
        addr: header_ipa,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 1,
    }
    .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 0)
    .unwrap();
    VirtqDescriptor {
        addr: data_ipa,
        len: 512,
        flags: VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
        next: 2,
    }
    .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 1)
    .unwrap();
    VirtqDescriptor {
        addr: status_ipa,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 2)
    .unwrap();
    memory
        .write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)
        .unwrap();
    memory
        .write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 0)
        .unwrap();

    let completion = block
        .complete_next_available_block_request(&mut memory)
        .unwrap();

    assert_eq!(
        completion,
        VirtioBlockRequestCompletion {
            descriptor_index: 0,
            request_type: VIRTIO_BLK_T_IN,
            sector,
            data_bytes: 512,
            status: VIRTIO_BLK_S_OK,
            used_index: 1,
            interrupt_status: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
        }
    );
    assert_eq!(block.completed_requests, 1);
    assert_eq!(
        memory.read_bytes(data_ipa, 8).unwrap(),
        (0..8)
            .map(|offset| synthetic_block_byte(sector, offset))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        memory.read_bytes(status_ipa, 1).unwrap(),
        vec![VIRTIO_BLK_S_OK]
    );
    assert_eq!(
        memory
            .read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 4)
            .unwrap(),
        0
    );
    assert_eq!(
        memory
            .read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)
            .unwrap(),
        513
    );
    assert_eq!(
        memory
            .read_u16(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 2)
            .unwrap(),
        1
    );
    assert_eq!(
        block.handle(MmioAccess::read(
            block_base + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            4
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert_eq!(
        block.complete_next_available_block_request(&mut memory),
        Err(VirtioBlockRequestError::NoAvailableRequest)
    );
}
