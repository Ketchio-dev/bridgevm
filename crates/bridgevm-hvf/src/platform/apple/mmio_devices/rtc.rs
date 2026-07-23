//! Split out of mmio_devices.rs by responsibility.

use super::super::*;
use crate::*;

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
