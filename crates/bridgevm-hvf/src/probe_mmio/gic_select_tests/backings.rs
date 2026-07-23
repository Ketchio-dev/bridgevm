//! virtio-block probe backings and the RTC continuation probe.

use crate::probe_mmio::*;
use crate::*;

#[test]
fn virtio_block_request_model_probe_reports_completion() {
    let probe = probe_virtio_block_request_model();
    let output = probe.render_text();

    assert!(probe.configured_via_mmio);
    assert!(probe.configured_via_mmio_bus);
    assert!(probe.queue_notified);
    assert_eq!(probe.queue_notify_value, Some(0));
    assert!(probe.completed_via_device_bus);
    assert!(probe.completed);
    assert_eq!(probe.descriptor_index, Some(0));
    assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
    assert_eq!(probe.sector, Some(7));
    assert_eq!(probe.data_bytes, Some(512));
    assert_eq!(
        probe.data_prefix,
        (0..8)
            .map(|offset| synthetic_block_byte(7, offset))
            .collect::<Vec<_>>()
    );
    assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
    assert_eq!(probe.used_index, Some(1));
    assert_eq!(probe.used_len, Some(513));
    assert_eq!(
        probe.interrupt_status,
        Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(probe.blockers.is_empty());
    assert!(output.contains("VirtIO block request model probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(
        output.contains("Guest execution: not entered; in-memory VirtIO block descriptor chain")
    );
    assert!(output.contains("Configured via MMIO: true"));
    assert!(output.contains("Configured via MMIO bus: true"));
    assert!(output.contains("Queue notified: true"));
    assert!(output.contains("Queue notify value: 0x0"));
    assert!(output.contains("Completed via device bus: true"));
    assert!(output.contains("Completed: true"));
    assert!(output.contains("Descriptor index: 0x0"));
    assert!(output.contains("Request type: 0x0"));
    assert!(output.contains("Sector: 0x7"));
    assert!(output.contains("Data bytes: 0x200"));
    assert!(output.contains("Data prefix: 0x0708090a0b0c0d0e"));
    assert!(output.contains("Status byte: 0x0"));
    assert!(output.contains("Used index: 0x1"));
    assert!(output.contains("Used length: 0x201"));
    assert!(output.contains("Interrupt status: 0x1"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn virtio_block_file_backing_probe_reads_from_host_file() {
    let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
    let sector_start = (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
    for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
        disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
    }
    let path = std::env::temp_dir().join(format!(
        "bridgevm-hvf-file-backed-{}-{}.img",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, &disk).unwrap();

    let probe = probe_virtio_block_file_backing(path.clone());
    let output = probe.render_text();
    let _ = std::fs::remove_file(&path);

    assert_eq!(probe.disk_path, path);
    assert_eq!(probe.backing_kind, "host-file");
    assert!(probe.configured_via_mmio);
    assert!(probe.configured_via_mmio_bus);
    assert!(probe.queue_notified);
    assert_eq!(probe.queue_notify_value, Some(0));
    assert!(probe.completed_via_device_bus);
    assert!(probe.completed);
    assert_eq!(probe.descriptor_index, Some(0));
    assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
    assert_eq!(probe.sector, Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR));
    assert_eq!(probe.byte_offset, Some(0xe00));
    assert_eq!(
        probe.data_bytes,
        Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
    );
    assert_eq!(
        probe.data_prefix,
        vec![0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7]
    );
    assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
    assert_eq!(probe.used_index, Some(1));
    assert_eq!(probe.used_len, Some(513));
    assert_eq!(
        probe.interrupt_status,
        Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(probe.blockers.is_empty());
    assert!(output.contains("VirtIO block file backing probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output
        .contains("Guest execution: not entered; host file-backed VirtIO block descriptor chain"));
    assert!(output.contains("Backing kind: host-file"));
    assert!(output.contains("Configured via MMIO: true"));
    assert!(output.contains("Configured via MMIO bus: true"));
    assert!(output.contains("Queue notified: true"));
    assert!(output.contains("Completed via device bus: true"));
    assert!(output.contains("Completed: true"));
    assert!(output.contains("Descriptor index: 0x0"));
    assert!(output.contains("Request type: 0x0"));
    assert!(output.contains("Sector: 0x7"));
    assert!(output.contains("Byte offset: 0xe00"));
    assert!(output.contains("Data bytes: 0x200"));
    assert!(output.contains("Data prefix: 0xa0a1a2a3a4a5a6a7"));
    assert!(output.contains("Status byte: 0x0"));
    assert!(output.contains("Used index: 0x1"));
    assert!(output.contains("Used length: 0x201"));
    assert!(output.contains("Interrupt status: 0x1"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn virtio_block_writable_file_backing_probe_writes_flushes_and_persists() {
    let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
    let sector_start = (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
    for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
        disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
    }
    let path = std::env::temp_dir().join(format!(
        "bridgevm-hvf-writable-file-backed-{}-{}.img",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, &disk).unwrap();

    let probe = probe_virtio_block_writable_file_backing(path.clone());
    let output = probe.render_text();
    let _ = std::fs::remove_file(&path);

    assert_eq!(probe.disk_path, path);
    assert_eq!(probe.backing_kind, "host-file-writable");
    assert!(probe.configured_via_mmio);
    assert!(probe.configured_via_mmio_bus);
    assert!(probe.queue_notified);
    assert_eq!(probe.queue_notify_value, Some(0));
    assert_eq!(
        probe.initial_read_prefix,
        vec![0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7]
    );
    assert!(probe.write_completed);
    assert_eq!(probe.write_request_type, Some(VIRTIO_BLK_T_OUT));
    assert_eq!(
        probe.write_sector,
        Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR)
    );
    assert_eq!(probe.write_byte_offset, Some(0xe00));
    assert_eq!(
        probe.write_data_bytes,
        Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
    );
    assert_eq!(
        probe.write_data_prefix,
        vec![0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
    );
    assert_eq!(probe.write_status, Some(VIRTIO_BLK_S_OK));
    assert_eq!(probe.write_used_index, Some(2));
    assert_eq!(probe.write_used_len, Some(VIRTIO_BLOCK_STATUS_BYTES));
    assert!(probe.flush_completed);
    assert_eq!(probe.flush_request_type, Some(VIRTIO_BLK_T_FLUSH));
    assert_eq!(probe.flush_status, Some(VIRTIO_BLK_S_OK));
    assert_eq!(probe.flush_used_index, Some(3));
    assert_eq!(probe.flush_used_len, Some(VIRTIO_BLOCK_STATUS_BYTES));
    assert_eq!(
        probe.persisted_data_prefix,
        vec![0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
    );
    assert_eq!(
        probe.interrupt_status,
        Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(probe.blockers.is_empty());
    assert!(output.contains("VirtIO block writable file backing probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output.contains(
        "Guest execution: not entered; host file-backed VirtIO block write/flush persistence descriptor chain"
    ));
    assert!(output.contains("Backing kind: host-file-writable"));
    assert!(output.contains("Configured via MMIO: true"));
    assert!(output.contains("Configured via MMIO bus: true"));
    assert!(output.contains("Queue notified: true"));
    assert!(output.contains("Initial read data prefix: 0xa0a1a2a3a4a5a6a7"));
    assert!(output.contains("Write completed: true"));
    assert!(output.contains("Write request type: 0x1"));
    assert!(output.contains("Write sector: 0x7"));
    assert!(output.contains("Write byte offset: 0xe00"));
    assert!(output.contains("Write data bytes: 0x200"));
    assert!(output.contains("Write data prefix: 0xe0e1e2e3e4e5e6e7"));
    assert!(output.contains("Write status byte: 0x0"));
    assert!(output.contains("Write used index: 0x2"));
    assert!(output.contains("Write used length: 0x1"));
    assert!(output.contains("Flush completed: true"));
    assert!(output.contains("Flush request type: 0x4"));
    assert!(output.contains("Flush status byte: 0x0"));
    assert!(output.contains("Flush used index: 0x3"));
    assert!(output.contains("Flush used length: 0x1"));
    assert!(output.contains("Persisted data prefix: 0xe0e1e2e3e4e5e6e7"));
    assert!(output.contains("Interrupt status: 0x1"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn virtio_block_iso_backing_probe_reads_from_read_only_iso() {
    let mut iso = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
    let sector_start = (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
    for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
        iso[sector_start + offset] = 0xc0_u8.wrapping_add(offset as u8);
    }
    let path = std::env::temp_dir().join(format!(
        "bridgevm-hvf-iso-backed-{}-{}.iso",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, &iso).unwrap();

    let probe = probe_virtio_block_iso_backing(path.clone());
    let output = probe.render_text();
    let _ = std::fs::remove_file(&path);

    assert_eq!(probe.iso_path, path);
    assert_eq!(probe.backing_kind, "host-iso-readonly");
    assert_eq!(probe.media_mode, "read-only");
    assert!(probe.configured_via_mmio);
    assert!(probe.configured_via_mmio_bus);
    assert!(probe.queue_notified);
    assert_eq!(probe.queue_notify_value, Some(0));
    assert!(probe.completed_via_device_bus);
    assert!(probe.completed);
    assert_eq!(probe.descriptor_index, Some(0));
    assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
    assert_eq!(probe.sector, Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR));
    assert_eq!(probe.byte_offset, Some(0xe00));
    assert_eq!(
        probe.data_bytes,
        Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
    );
    assert_eq!(
        probe.data_prefix,
        vec![0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7]
    );
    assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
    assert_eq!(probe.used_index, Some(1));
    assert_eq!(probe.used_len, Some(513));
    assert_eq!(
        probe.interrupt_status,
        Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(probe.readonly_write_rejected);
    assert_eq!(probe.readonly_write_status, Some(VIRTIO_BLK_S_IOERR));
    assert_eq!(probe.readonly_write_used_index, Some(2));
    assert_eq!(
        probe.readonly_write_used_len,
        Some(VIRTIO_BLOCK_STATUS_BYTES)
    );
    assert_eq!(
        probe.readonly_write_interrupt_status,
        Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
    );
    assert!(probe.blockers.is_empty());
    assert!(output.contains("VirtIO block ISO backing probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output.contains(
        "Guest execution: not entered; read-only ISO-backed VirtIO block descriptor chain"
    ));
    assert!(output.contains(&format!("ISO path: {}", path.display())));
    assert!(output.contains("Backing kind: host-iso-readonly"));
    assert!(output.contains("Media mode: read-only"));
    assert!(output.contains("Configured via MMIO: true"));
    assert!(output.contains("Configured via MMIO bus: true"));
    assert!(output.contains("Queue notified: true"));
    assert!(output.contains("Completed via device bus: true"));
    assert!(output.contains("Completed: true"));
    assert!(output.contains("Descriptor index: 0x0"));
    assert!(output.contains("Request type: 0x0"));
    assert!(output.contains("Sector: 0x7"));
    assert!(output.contains("Byte offset: 0xe00"));
    assert!(output.contains("Data bytes: 0x200"));
    assert!(output.contains("Data prefix: 0xc0c1c2c3c4c5c6c7"));
    assert!(output.contains("Status byte: 0x0"));
    assert!(output.contains("Used index: 0x1"));
    assert!(output.contains("Used length: 0x201"));
    assert!(output.contains("Interrupt status: 0x1"));
    assert!(output.contains("Read-only write rejected: true"));
    assert!(output.contains("Read-only write status byte: 0x1"));
    assert!(output.contains("Read-only write used index: 0x2"));
    assert!(output.contains("Read-only write used length: 0x1"));
    assert!(output.contains("Read-only write interrupt status: 0x1"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn mmio_rtc_device_probe_render_records_multi_device_continuation() {
    let probe = HvfMmioRtcDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created: true,
        memory_allocated: true,
        memory_mapped: true,
        vcpu_created: true,
        pc_set: true,
        cpsr_set: true,
        rtc_address_register_set: true,
        device_bus_created: true,
        device_bus_device_count: 2,
        first_run_attempted: true,
        rtc_exit_observed: true,
        rtc_handled_by_device: true,
        rtc_value_injected: true,
        pc_read_after_rtc_exit: true,
        pc_advanced: true,
        second_run_attempted: true,
        continuation_exit_observed: true,
        rtc_value_preserved: true,
        watchdog_cancel_fired: false,
        vcpu_destroyed: true,
        memory_unmapped: true,
        vm_destroyed: true,
        memory_deallocated: true,
        host: HvfHostCapabilities {
            available: true,
            host: "macos-aarch64",
            default_ipa_bits: Some(36),
            max_ipa_bits: Some(40),
            el2_supported: Some(true),
            blockers: Vec::new(),
        },
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: 0x4000_0000,
        uart_ipa: 0x5000_0000,
        rtc_ipa: 0x5000_1000,
        bytes: 16 * 1024,
        instructions: "LDR X0, [X1]; HVC #0",
        rtc_value: 0x2026_0618,
        vm_create_status: Some(0),
        allocate_status: Some(0),
        map_status: Some(0),
        vcpu_create_status: Some(0),
        pc_set_status: Some(0),
        cpsr_set_status: Some(0),
        rtc_address_register_set_status: Some(0),
        first_run_status: Some(0),
        rtc_exit_reason: Some(1),
        rtc_exit_syndrome: Some(0x93c0_8006),
        rtc_exit_virtual_address: Some(0x5000_1000),
        rtc_exit_physical_address: Some(0x5000_1000),
        first_watchdog_cancel_status: None,
        rtc_value_set_status: Some(0),
        pc_read_status: Some(0),
        pc_after_rtc_exit: Some(0x4000_0000),
        pc_advance_status: Some(0),
        second_run_status: Some(0),
        continuation_exit_reason: Some(1),
        continuation_exit_syndrome: Some(0x5a00_0000),
        continuation_exit_virtual_address: Some(0),
        continuation_exit_physical_address: Some(0),
        second_watchdog_cancel_status: None,
        rtc_value_after_continue_status: Some(0),
        rtc_value_after_continue: Some(0x2026_0618),
        vcpu_destroy_status: Some(0),
        unmap_status: Some(0),
        vm_destroy_status: Some(0),
        deallocate_status: Some(0),
        blockers: Vec::new(),
    };
    let output = probe.render_text();

    assert!(output.contains("Allowed: true"));
    assert!(output.contains("Attempted: true"));
    assert!(output.contains("Device models: PL011 UART skeleton; PL031 RTC skeleton"));
    assert!(output.contains("Device bus created: true"));
    assert!(output.contains("Device bus device count: 2"));
    assert!(output.contains("RTC exit observed: true"));
    assert!(output.contains("RTC handled by device: true"));
    assert!(output.contains("RTC value injected: true"));
    assert!(output.contains("PC advanced: true"));
    assert!(output.contains("Continuation exit observed: true"));
    assert!(output.contains("RTC value preserved: true"));
    assert!(output.contains("RTC exit syndrome: 0x93c08006"));
    assert!(output.contains("RTC exit virtual address: 0x50001000"));
    assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
    assert!(output.contains("RTC value after continue: 0x20260618"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains('%'));
}
