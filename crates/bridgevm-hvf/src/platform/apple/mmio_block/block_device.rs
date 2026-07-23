//! Split out of mmio_block.rs by responsibility.

use super::super::*;
use super::*;
use crate::*;

pub fn probe_hvf_mmio_block_device(
    allow_device: bool,
    host: HvfHostCapabilities,
) -> HvfMmioBlockDeviceProbe {
    let mut blockers = Vec::new();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE=1 or pass --allow-device to emulate VirtIO-MMIO block identity reads through the MMIO bus".to_string(),
        );
        return mmio_block_device_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_block_device_probe_result(true, false, host, blockers);
    }

    let specs = block_identity_register_specs();
    let mut register_reads = block_register_probe_defaults();
    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut register_address_registers_set = false;
    let mut continuation_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut vendor_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut register_address_registers_set_status = vec![None; specs.len()];
    let mut continuation_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut continuation_watchdog_cancel_status = None;
    let mut vendor_value_after_continue_status = None;
    let mut vendor_value_after_continue = None;
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
    mmio_bus.attach(Box::new(VirtioMmioBlockDevice::new(BLOCK_MMIO_IPA)));
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
            for (index, spec) in specs.iter().enumerate() {
                let instruction = spec.instruction.to_le_bytes();
                unsafe {
                    ptr::copy_nonoverlapping(
                        instruction.as_ptr(),
                        memory.cast::<u8>().add(index * instruction.len()),
                        instruction.len(),
                    );
                }
            }
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(specs.len() * hvc.len()),
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
        for (index, spec) in specs.iter().enumerate() {
            let status = unsafe { hv_vcpu_set_reg(vcpu, spec.address_reg, spec.ipa) };
            register_address_registers_set_status[index] = Some(status);
            if status != HV_SUCCESS {
                blockers.push(format!(
                    "hv_vcpu_set_reg(X{} {}) failed: {status:#x}",
                    index + 1,
                    spec.name
                ));
            }
        }
        register_address_registers_set = register_address_registers_set_status
            .iter()
            .all(|status| *status == Some(HV_SUCCESS));
    }

    let mut can_continue = vcpu_created && pc_set && cpsr_set && register_address_registers_set;
    for (index, spec) in specs.iter().enumerate() {
        if !can_continue {
            break;
        }

        let read = &mut register_reads[index];
        read.run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        read.run_status = Some(observation.run_status);
        read.exit_reason = observation.exit_reason;
        read.exit_syndrome = observation.exit_syndrome;
        read.exit_virtual_address = observation.exit_virtual_address;
        read.exit_physical_address = observation.exit_physical_address;
        read.watchdog_cancel_status = observation.watchdog_cancel_status;
        if read.watchdog_cancel_status.is_some() {
            blockers.push(format!(
                "VirtIO block {} read watchdog fired before exception exit",
                spec.name
            ));
            can_continue = false;
        }

        if observation.run_status == HV_SUCCESS {
            if read.exit_reason.is_none() {
                blockers.push(format!(
                    "VirtIO block {} read returned success without an exit info pointer",
                    spec.name
                ));
                can_continue = false;
            } else {
                read.exit_observed = read.exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (read.exit_virtual_address == Some(spec.ipa)
                        || read.exit_physical_address == Some(spec.ipa)
                        || read.exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !read.exit_observed {
                    blockers.push(format!(
                        "VirtIO block {} read did not exit at IPA {:#x}",
                        spec.name, spec.ipa
                    ));
                    can_continue = false;
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block {} hv_vcpu_run failed: {:#x}",
                spec.name, observation.run_status
            ));
            can_continue = false;
        }

        if read.exit_observed {
            match mmio_bus.dispatch(MmioAccess::read(spec.ipa, 4)) {
                MmioAction::ReadValue(value) => {
                    read.handled_by_device = true;
                    if value != spec.value {
                        blockers.push(format!(
                            "VirtIO block {} returned unexpected value: {value:#x}",
                            spec.name
                        ));
                        can_continue = false;
                    }
                    let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                    read.value_set_status = Some(status);
                    read.value_injected = status == HV_SUCCESS;
                    if !read.value_injected {
                        blockers.push(format!(
                            "hv_vcpu_set_reg(X0 {}) failed: {status:#x}",
                            spec.name
                        ));
                        can_continue = false;
                    }
                }
                MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                    blockers.push(format!(
                        "VirtIO block {} read was not handled by the MMIO device bus at {:#x}",
                        spec.name, spec.ipa
                    ));
                    can_continue = false;
                }
            }
        }

        if read.exit_observed {
            let mut pc = 0;
            let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
            read.pc_read_status = Some(status);
            read.pc_read_after_exit = status == HV_SUCCESS;
            if read.pc_read_after_exit {
                read.pc_after_exit = Some(pc);
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_reg(PC after VirtIO block {}) failed: {status:#x}",
                    spec.name
                ));
                can_continue = false;
            }
        }

        if read.pc_read_after_exit && read.value_injected {
            let next_pc = PROBE_IPA_START + ((index as u64 + 1) * 4);
            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, next_pc) };
            read.pc_advance_status = Some(status);
            read.pc_advanced = status == HV_SUCCESS;
            if !read.pc_advanced {
                blockers.push(format!(
                    "hv_vcpu_set_reg(PC after VirtIO block {}) failed: {status:#x}",
                    spec.name
                ));
                can_continue = false;
            }
        }
    }

    if register_reads.iter().all(|read| read.pc_advanced) {
        continuation_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        continuation_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        continuation_watchdog_cancel_status = observation.watchdog_cancel_status;
        if continuation_watchdog_cancel_status.is_some() {
            blockers.push("VirtIO block continuation watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "VirtIO block continuation returned success without an exit info pointer"
                        .to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "VirtIO block continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| format!("{value:#x}")
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        vendor_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            vendor_value_after_continue = Some(value);
            vendor_value_preserved = value == VIRTIO_MMIO_VENDOR_ID_VALUE;
            if !vendor_value_preserved {
                blockers.push(format!(
                    "VirtIO block vendor value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after VirtIO block continue) failed: {status:#x}"
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

    let watchdog_cancel_fired = register_reads
        .iter()
        .any(|read| read.watchdog_cancel_status.is_some())
        || continuation_watchdog_cancel_status.is_some();

    HvfMmioBlockDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        register_address_registers_set,
        device_bus_created,
        device_bus_device_count,
        register_reads,
        continuation_run_attempted,
        continuation_exit_observed,
        vendor_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR W0 magic/version/device/vendor; HVC #0",
        magic_value: VIRTIO_MMIO_MAGIC_VALUE,
        version_value: VIRTIO_MMIO_VERSION_VALUE,
        device_id_value: VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE,
        vendor_id_value: VIRTIO_MMIO_VENDOR_ID_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        register_address_registers_set_status,
        continuation_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        continuation_watchdog_cancel_status,
        vendor_value_after_continue_status,
        vendor_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockQueueAccessKind {
    Read,
    Write,
}

impl BlockQueueAccessKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BlockQueueStepSpec {
    pub(crate) name: &'static str,
    pub(crate) access: BlockQueueAccessKind,
    pub(crate) ipa: u64,
    pub(crate) expected_value: Option<u64>,
    pub(crate) write_value: Option<u64>,
    pub(crate) instruction: u32,
}
