//! MMIO bus routing for the serial device and unmapped accesses.

use crate::probe_mmio::*;
use crate::*;

#[test]
fn mmio_serial_device_probe_render_records_three_exit_device_loop() {
    let probe = HvfMmioSerialDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created: true,
        memory_allocated: true,
        memory_mapped: true,
        vcpu_created: true,
        pc_set: true,
        cpsr_set: true,
        write_value_register_set: true,
        data_address_register_set: true,
        status_address_register_set: true,
        device_bus_created: true,
        device_bus_device_count: 1,
        write_run_attempted: true,
        write_exit_observed: true,
        write_handled_by_device: true,
        write_value_captured: true,
        pc_advanced_after_write: true,
        status_run_attempted: true,
        status_exit_observed: true,
        status_handled_by_device: true,
        status_value_injected: true,
        pc_advanced_after_status: true,
        continuation_run_attempted: true,
        continuation_exit_observed: true,
        status_value_preserved: true,
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
        device_model: PL011_UART_MODEL,
        code_ipa_start: 0x4000_0000,
        data_ipa: 0x5000_0000,
        status_ipa: 0x5000_0018,
        bytes: 16 * 1024,
        instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
        serial_write_value: 0x41,
        serial_status_value: 0x90,
        captured_write_value: Some(0x41),
        captured_byte: Some(0x41),
        vm_create_status: Some(0),
        allocate_status: Some(0),
        map_status: Some(0),
        vcpu_create_status: Some(0),
        pc_set_status: Some(0),
        cpsr_set_status: Some(0),
        write_value_register_set_status: Some(0),
        data_address_register_set_status: Some(0),
        status_address_register_set_status: Some(0),
        write_run_status: Some(0),
        write_exit_reason: Some(1),
        write_exit_syndrome: Some(0x93c0_8046),
        write_exit_virtual_address: Some(0x5000_0000),
        write_exit_physical_address: Some(0x5000_0000),
        write_watchdog_cancel_status: None,
        write_value_capture_status: Some(0),
        pc_read_after_write_status: Some(0),
        pc_after_write_exit: Some(0x4000_0000),
        pc_advance_after_write_status: Some(0),
        status_run_status: Some(0),
        status_exit_reason: Some(1),
        status_exit_syndrome: Some(0x93c0_8006),
        status_exit_virtual_address: Some(0x5000_0018),
        status_exit_physical_address: Some(0x5000_0018),
        status_watchdog_cancel_status: None,
        status_value_set_status: Some(0),
        pc_read_after_status_status: Some(0),
        pc_after_status_exit: Some(0x4000_0004),
        pc_advance_after_status_status: Some(0),
        continuation_run_status: Some(0),
        continuation_exit_reason: Some(1),
        continuation_exit_syndrome: Some(0x5a00_0000),
        continuation_exit_virtual_address: Some(0),
        continuation_exit_physical_address: Some(0),
        continuation_watchdog_cancel_status: None,
        status_value_after_continue_status: Some(0),
        status_value_after_continue: Some(0x90),
        vcpu_destroy_status: Some(0),
        unmap_status: Some(0),
        vm_destroy_status: Some(0),
        deallocate_status: Some(0),
        blockers: Vec::new(),
    };
    let output = probe.render_text();

    assert!(output.contains("Allowed: true"));
    assert!(output.contains("Attempted: true"));
    assert!(output.contains("Device model: PL011 UART skeleton"));
    assert!(output.contains("Device bus created: true"));
    assert!(output.contains("Device bus device count: 1"));
    assert!(output.contains("Write exit observed: true"));
    assert!(output.contains("Write handled by device: true"));
    assert!(output.contains("Write value captured: true"));
    assert!(output.contains("PC advanced after write: true"));
    assert!(output.contains("Status exit observed: true"));
    assert!(output.contains("Status handled by device: true"));
    assert!(output.contains("Status value injected: true"));
    assert!(output.contains("PC advanced after status: true"));
    assert!(output.contains("Continuation exit observed: true"));
    assert!(output.contains("Status value preserved: true"));
    assert!(output.contains("Captured write value: 0x41"));
    assert!(output.contains("Captured byte: 0x41"));
    assert!(output.contains("Write exit syndrome: 0x93c08046"));
    assert!(output.contains("Write exit virtual address: 0x50000000"));
    assert!(output.contains("Status exit syndrome: 0x93c08006"));
    assert!(output.contains("Status exit virtual address: 0x50000018"));
    assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
    assert!(output.contains("Status value after continue: 0x90"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains('%'));
}

#[test]
fn mmio_bus_routes_probe_serial_data_write() {
    let mut bus = MmioBus::default();
    bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

    assert_eq!(bus.device_count(), 1);
    assert_eq!(
        bus.dispatch(MmioAccess::write(0x5000_0000, 0x141, 8)),
        MmioAction::WriteAccepted {
            value: 0x141,
            byte: 0x41,
        }
    );
}

#[test]
fn mmio_bus_routes_probe_serial_status_read() {
    let mut bus = MmioBus::default();
    bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

    assert_eq!(
        bus.dispatch(MmioAccess::read(0x5000_0018, 8)),
        MmioAction::ReadValue(0x90)
    );
}

#[test]
fn mmio_bus_reports_unmapped_access_as_unhandled() {
    let mut bus = MmioBus::default();
    bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

    assert_eq!(
        bus.dispatch(MmioAccess::read(0x6000_0000, 8)),
        MmioAction::Unhandled
    );
}
