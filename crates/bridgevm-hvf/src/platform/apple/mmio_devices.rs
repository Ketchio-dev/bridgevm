//! Live PL011 serial and PL031 RTC device probes.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_hvf_mmio_serial_device(
    allow_device: bool,
    host: HvfHostCapabilities,
) -> HvfMmioSerialDeviceProbe {
    let mut blockers = Vec::new();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE=1 or pass --allow-device to emulate one serial data write, one status read, and one HVC continuation".to_string(),
        );
        return mmio_serial_device_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_serial_device_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut write_value_register_set = false;
    let mut data_address_register_set = false;
    let mut status_address_register_set = false;
    let mut write_run_attempted = false;
    let mut write_exit_observed = false;
    let mut write_handled_by_device = false;
    let mut write_value_captured = false;
    let mut pc_read_after_write = false;
    let mut pc_advanced_after_write = false;
    let mut status_run_attempted = false;
    let mut status_exit_observed = false;
    let mut status_handled_by_device = false;
    let mut status_value_injected = false;
    let mut pc_read_after_status = false;
    let mut pc_advanced_after_status = false;
    let mut continuation_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut status_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut write_value_register_set_status = None;
    let mut data_address_register_set_status = None;
    let mut status_address_register_set_status = None;
    let mut write_run_status = None;
    let mut write_exit_reason = None;
    let mut write_exit_syndrome = None;
    let mut write_exit_virtual_address = None;
    let mut write_exit_physical_address = None;
    let mut write_watchdog_cancel_status = None;
    let mut write_value_capture_status = None;
    let mut captured_write_value = None;
    let mut captured_byte = None;
    let mut pc_read_after_write_status = None;
    let mut pc_after_write_exit = None;
    let mut pc_advance_after_write_status = None;
    let mut status_run_status = None;
    let mut status_exit_reason = None;
    let mut status_exit_syndrome = None;
    let mut status_exit_virtual_address = None;
    let mut status_exit_physical_address = None;
    let mut status_watchdog_cancel_status = None;
    let mut status_value_set_status = None;
    let mut pc_read_after_status_status = None;
    let mut pc_after_status_exit = None;
    let mut pc_advance_after_status_status = None;
    let mut continuation_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut continuation_watchdog_cancel_status = None;
    let mut status_value_after_continue_status = None;
    let mut status_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let mut mmio_bus = MmioBus::default();
    mmio_bus.attach(Box::new(Pl011UartDevice::new(
        PROBE_MMIO_IPA,
        SERIAL_MMIO_STATUS_VALUE,
    )));
    let device_bus_created = true;
    let device_bus_device_count = mmio_bus.device_count();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let store_data = AARCH64_STR_X0_TO_X1.to_le_bytes();
            let load_status = AARCH64_LDR_X0_FROM_X2.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    store_data.as_ptr(),
                    memory.cast::<u8>(),
                    store_data.len(),
                );
                ptr::copy_nonoverlapping(
                    load_status.as_ptr(),
                    memory.cast::<u8>().add(store_data.len()),
                    load_status.len(),
                );
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory
                        .cast::<u8>()
                        .add(store_data.len() + load_status.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, SERIAL_MMIO_WRITE_VALUE) };
        write_value_register_set_status = Some(status);
        write_value_register_set = status == HV_SUCCESS;
        if !write_value_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X0) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, SERIAL_MMIO_DATA_IPA) };
        data_address_register_set_status = Some(status);
        data_address_register_set = status == HV_SUCCESS;
        if !data_address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X2, SERIAL_MMIO_STATUS_IPA) };
        status_address_register_set_status = Some(status);
        status_address_register_set = status == HV_SUCCESS;
        if !status_address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X2) failed: {status:#x}"));
        }
    }

    if vcpu_created
        && pc_set
        && cpsr_set
        && write_value_register_set
        && data_address_register_set
        && status_address_register_set
    {
        write_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        write_run_status = Some(observation.run_status);
        write_exit_reason = observation.exit_reason;
        write_exit_syndrome = observation.exit_syndrome;
        write_exit_virtual_address = observation.exit_virtual_address;
        write_exit_physical_address = observation.exit_physical_address;
        write_watchdog_cancel_status = observation.watchdog_cancel_status;
        if write_watchdog_cancel_status.is_some() {
            blockers.push("serial data write watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if write_exit_reason.is_none() {
                blockers.push(
                    "serial data write returned success without an exit info pointer".to_string(),
                );
            } else {
                write_exit_observed = write_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (write_exit_virtual_address == Some(SERIAL_MMIO_DATA_IPA)
                        || write_exit_physical_address == Some(SERIAL_MMIO_DATA_IPA)
                        || write_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !write_exit_observed {
                    blockers.push(format!(
                        "serial data write did not exit at data IPA {SERIAL_MMIO_DATA_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "serial data write hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if write_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        write_value_capture_status = Some(status);
        if status == HV_SUCCESS {
            match mmio_bus.dispatch(MmioAccess::write(SERIAL_MMIO_DATA_IPA, value, 8)) {
                MmioAction::WriteAccepted { value, byte } => {
                    write_handled_by_device = true;
                    captured_write_value = Some(value);
                    captured_byte = Some(byte);
                    write_value_captured = value == SERIAL_MMIO_WRITE_VALUE;
                    if !write_value_captured {
                        blockers.push(format!(
                            "serial data write captured unexpected value: {value:#x}"
                        ));
                    }
                }
                MmioAction::Unhandled | MmioAction::ReadValue(_) => {
                    blockers.push(format!(
                        "serial data write was not handled by the MMIO device bus at {SERIAL_MMIO_DATA_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
        }
    }

    if write_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_after_write_status = Some(status);
        pc_read_after_write = status == HV_SUCCESS;
        if pc_read_after_write {
            pc_after_write_exit = Some(pc);
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(PC after write) failed: {status:#x}"
            ));
        }
    }

    if pc_read_after_write && write_value_captured {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_after_write_status = Some(status);
        pc_advanced_after_write = status == HV_SUCCESS;
        if !pc_advanced_after_write {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced_after_write {
        status_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        status_run_status = Some(observation.run_status);
        status_exit_reason = observation.exit_reason;
        status_exit_syndrome = observation.exit_syndrome;
        status_exit_virtual_address = observation.exit_virtual_address;
        status_exit_physical_address = observation.exit_physical_address;
        status_watchdog_cancel_status = observation.watchdog_cancel_status;
        if status_watchdog_cancel_status.is_some() {
            blockers.push("serial status read watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if status_exit_reason.is_none() {
                blockers.push(
                    "serial status read returned success without an exit info pointer".to_string(),
                );
            } else {
                status_exit_observed = status_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (status_exit_virtual_address == Some(SERIAL_MMIO_STATUS_IPA)
                        || status_exit_physical_address == Some(SERIAL_MMIO_STATUS_IPA)
                        || status_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !status_exit_observed {
                    blockers.push(format!(
                        "serial status read did not exit at status IPA {SERIAL_MMIO_STATUS_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "serial status read hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if status_exit_observed {
        match mmio_bus.dispatch(MmioAccess::read(SERIAL_MMIO_STATUS_IPA, 8)) {
            MmioAction::ReadValue(value) => {
                status_handled_by_device = true;
                let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                status_value_set_status = Some(status);
                status_value_injected = status == HV_SUCCESS;
                if value != SERIAL_MMIO_STATUS_VALUE {
                    blockers.push(format!(
                        "serial status device returned unexpected value: {value:#x}"
                    ));
                }
                if !status_value_injected {
                    blockers.push(format!("hv_vcpu_set_reg(X0 status) failed: {status:#x}"));
                }
            }
            MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                blockers.push(format!(
                    "serial status read was not handled by the MMIO device bus at {SERIAL_MMIO_STATUS_IPA:#x}"
                ));
            }
        }
    }

    if status_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_after_status_status = Some(status);
        pc_read_after_status = status == HV_SUCCESS;
        if pc_read_after_status {
            pc_after_status_exit = Some(pc);
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(PC after status) failed: {status:#x}"
            ));
        }
    }

    if pc_read_after_status && status_value_injected {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 8) };
        pc_advance_after_status_status = Some(status);
        pc_advanced_after_status = status == HV_SUCCESS;
        if !pc_advanced_after_status {
            blockers.push(format!("hv_vcpu_set_reg(PC + 8) failed: {status:#x}"));
        }
    }

    if pc_advanced_after_status {
        continuation_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        continuation_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        continuation_watchdog_cancel_status = observation.watchdog_cancel_status;
        if continuation_watchdog_cancel_status.is_some() {
            blockers.push("serial continuation watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "serial continuation returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "serial continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "serial continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        status_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            status_value_after_continue = Some(value);
            status_value_preserved = value == SERIAL_MMIO_STATUS_VALUE;
            if !status_value_preserved {
                blockers.push(format!(
                    "serial status value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after continue) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired = write_watchdog_cancel_status.is_some()
        || status_watchdog_cancel_status.is_some()
        || continuation_watchdog_cancel_status.is_some();

    HvfMmioSerialDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        write_value_register_set,
        data_address_register_set,
        status_address_register_set,
        device_bus_created,
        device_bus_device_count,
        write_run_attempted,
        write_exit_observed,
        write_handled_by_device,
        write_value_captured,
        pc_advanced_after_write,
        status_run_attempted,
        status_exit_observed,
        status_handled_by_device,
        status_value_injected,
        pc_advanced_after_status,
        continuation_run_attempted,
        continuation_exit_observed,
        status_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_model: PL011_UART_MODEL,
        code_ipa_start: PROBE_IPA_START,
        data_ipa: SERIAL_MMIO_DATA_IPA,
        status_ipa: SERIAL_MMIO_STATUS_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
        serial_write_value: SERIAL_MMIO_WRITE_VALUE,
        serial_status_value: SERIAL_MMIO_STATUS_VALUE,
        captured_write_value,
        captured_byte,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        write_value_register_set_status,
        data_address_register_set_status,
        status_address_register_set_status,
        write_run_status,
        write_exit_reason,
        write_exit_syndrome,
        write_exit_virtual_address,
        write_exit_physical_address,
        write_watchdog_cancel_status,
        write_value_capture_status,
        pc_read_after_write_status,
        pc_after_write_exit,
        pc_advance_after_write_status,
        status_run_status,
        status_exit_reason,
        status_exit_syndrome,
        status_exit_virtual_address,
        status_exit_physical_address,
        status_watchdog_cancel_status,
        status_value_set_status,
        pc_read_after_status_status,
        pc_after_status_exit,
        pc_advance_after_status_status,
        continuation_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        continuation_watchdog_cancel_status,
        status_value_after_continue_status,
        status_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn mmio_serial_device_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioSerialDeviceProbe {
    HvfMmioSerialDeviceProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        write_value_register_set: false,
        data_address_register_set: false,
        status_address_register_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        write_run_attempted: false,
        write_exit_observed: false,
        write_handled_by_device: false,
        write_value_captured: false,
        pc_advanced_after_write: false,
        status_run_attempted: false,
        status_exit_observed: false,
        status_handled_by_device: false,
        status_value_injected: false,
        pc_advanced_after_status: false,
        continuation_run_attempted: false,
        continuation_exit_observed: false,
        status_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_model: PL011_UART_MODEL,
        code_ipa_start: PROBE_IPA_START,
        data_ipa: SERIAL_MMIO_DATA_IPA,
        status_ipa: SERIAL_MMIO_STATUS_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
        serial_write_value: SERIAL_MMIO_WRITE_VALUE,
        serial_status_value: SERIAL_MMIO_STATUS_VALUE,
        captured_write_value: None,
        captured_byte: None,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        write_value_register_set_status: None,
        data_address_register_set_status: None,
        status_address_register_set_status: None,
        write_run_status: None,
        write_exit_reason: None,
        write_exit_syndrome: None,
        write_exit_virtual_address: None,
        write_exit_physical_address: None,
        write_watchdog_cancel_status: None,
        write_value_capture_status: None,
        pc_read_after_write_status: None,
        pc_after_write_exit: None,
        pc_advance_after_write_status: None,
        status_run_status: None,
        status_exit_reason: None,
        status_exit_syndrome: None,
        status_exit_virtual_address: None,
        status_exit_physical_address: None,
        status_watchdog_cancel_status: None,
        status_value_set_status: None,
        pc_read_after_status_status: None,
        pc_after_status_exit: None,
        pc_advance_after_status_status: None,
        continuation_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        continuation_watchdog_cancel_status: None,
        status_value_after_continue_status: None,
        status_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_rtc_device(
    allow_device: bool,
    host: HvfHostCapabilities,
) -> HvfMmioRtcDeviceProbe {
    let mut blockers = Vec::new();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE=1 or pass --allow-device to emulate a PL031 RTC read through the multi-device MMIO bus".to_string(),
        );
        return mmio_rtc_device_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_rtc_device_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut rtc_address_register_set = false;
    let mut first_run_attempted = false;
    let mut rtc_exit_observed = false;
    let mut rtc_handled_by_device = false;
    let mut rtc_value_injected = false;
    let mut pc_read_after_rtc_exit = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut rtc_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut rtc_address_register_set_status = None;
    let mut first_run_status = None;
    let mut rtc_exit_reason = None;
    let mut rtc_exit_syndrome = None;
    let mut rtc_exit_virtual_address = None;
    let mut rtc_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut rtc_value_set_status = None;
    let mut pc_read_status = None;
    let mut pc_after_rtc_exit = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut rtc_value_after_continue_status = None;
    let mut rtc_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let mut mmio_bus = MmioBus::default();
    mmio_bus.attach(Box::new(Pl011UartDevice::new(
        PROBE_MMIO_IPA,
        SERIAL_MMIO_STATUS_VALUE,
    )));
    mmio_bus.attach(Box::new(Pl031RtcDevice::new(
        RTC_MMIO_IPA,
        RTC_MMIO_READ_VALUE,
    )));
    let device_bus_created = true;
    let device_bus_device_count = mmio_bus.device_count();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let load_rtc = AARCH64_LDR_X0_FROM_X1.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(load_rtc.as_ptr(), memory.cast::<u8>(), load_rtc.len());
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(load_rtc.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, RTC_MMIO_IPA) };
        rtc_address_register_set_status = Some(status);
        rtc_address_register_set = status == HV_SUCCESS;
        if !rtc_address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1 RTC) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && rtc_address_register_set {
        first_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(observation.run_status);
        rtc_exit_reason = observation.exit_reason;
        rtc_exit_syndrome = observation.exit_syndrome;
        rtc_exit_virtual_address = observation.exit_virtual_address;
        rtc_exit_physical_address = observation.exit_physical_address;
        first_watchdog_cancel_status = observation.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers.push("RTC read watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if rtc_exit_reason.is_none() {
                blockers.push("RTC read returned success without an exit info pointer".to_string());
            } else {
                rtc_exit_observed = rtc_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (rtc_exit_virtual_address == Some(RTC_MMIO_IPA)
                        || rtc_exit_physical_address == Some(RTC_MMIO_IPA)
                        || rtc_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !rtc_exit_observed {
                    blockers.push(format!(
                        "RTC read did not exit at RTC IPA {RTC_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "RTC read hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if rtc_exit_observed {
        match mmio_bus.dispatch(MmioAccess::read(RTC_MMIO_IPA, 8)) {
            MmioAction::ReadValue(value) => {
                rtc_handled_by_device = true;
                let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                rtc_value_set_status = Some(status);
                rtc_value_injected = status == HV_SUCCESS;
                if value != RTC_MMIO_READ_VALUE {
                    blockers.push(format!("RTC device returned unexpected value: {value:#x}"));
                }
                if !rtc_value_injected {
                    blockers.push(format!("hv_vcpu_set_reg(X0 RTC) failed: {status:#x}"));
                }
            }
            MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                blockers.push(format!(
                    "RTC read was not handled by the MMIO device bus at {RTC_MMIO_IPA:#x}"
                ));
            }
        }
    }

    if rtc_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_rtc_exit = status == HV_SUCCESS;
        if pc_read_after_rtc_exit {
            pc_after_rtc_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC after RTC) failed: {status:#x}"));
        }
    }

    if pc_read_after_rtc_exit && rtc_value_injected {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        second_watchdog_cancel_status = observation.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers.push("RTC continuation watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "RTC continuation returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "RTC continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| format!("{value:#x}")
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "RTC continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        rtc_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            rtc_value_after_continue = Some(value);
            rtc_value_preserved = value == RTC_MMIO_READ_VALUE;
            if !rtc_value_preserved {
                blockers.push(format!(
                    "RTC value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after RTC continue) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();

    HvfMmioRtcDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        rtc_address_register_set,
        device_bus_created,
        device_bus_device_count,
        first_run_attempted,
        rtc_exit_observed,
        rtc_handled_by_device,
        rtc_value_injected,
        pc_read_after_rtc_exit,
        pc_advanced,
        second_run_attempted,
        continuation_exit_observed,
        rtc_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        uart_ipa: SERIAL_MMIO_DATA_IPA,
        rtc_ipa: RTC_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        rtc_value: RTC_MMIO_READ_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        rtc_address_register_set_status,
        first_run_status,
        rtc_exit_reason,
        rtc_exit_syndrome,
        rtc_exit_virtual_address,
        rtc_exit_physical_address,
        first_watchdog_cancel_status,
        rtc_value_set_status,
        pc_read_status,
        pc_after_rtc_exit,
        pc_advance_status,
        second_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        second_watchdog_cancel_status,
        rtc_value_after_continue_status,
        rtc_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn mmio_rtc_device_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioRtcDeviceProbe {
    HvfMmioRtcDeviceProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        rtc_address_register_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        first_run_attempted: false,
        rtc_exit_observed: false,
        rtc_handled_by_device: false,
        rtc_value_injected: false,
        pc_read_after_rtc_exit: false,
        pc_advanced: false,
        second_run_attempted: false,
        continuation_exit_observed: false,
        rtc_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        uart_ipa: SERIAL_MMIO_DATA_IPA,
        rtc_ipa: RTC_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        rtc_value: RTC_MMIO_READ_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        rtc_address_register_set_status: None,
        first_run_status: None,
        rtc_exit_reason: None,
        rtc_exit_syndrome: None,
        rtc_exit_virtual_address: None,
        rtc_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        rtc_value_set_status: None,
        pc_read_status: None,
        pc_after_rtc_exit: None,
        pc_advance_status: None,
        second_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        rtc_value_after_continue_status: None,
        rtc_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BlockIdentityRegisterSpec {
    pub(crate) name: &'static str,
    pub(crate) ipa: u64,
    pub(crate) value: u64,
    pub(crate) address_reg: u32,
    pub(crate) instruction: u32,
}
