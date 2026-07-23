//! Live virtio-mmio block identity and queue probes.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) fn block_identity_register_specs() -> [BlockIdentityRegisterSpec; 4] {
    [
        BlockIdentityRegisterSpec {
            name: "magic",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
            value: VIRTIO_MMIO_MAGIC_VALUE,
            address_reg: HV_REG_X1,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockIdentityRegisterSpec {
            name: "version",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_VERSION_OFFSET,
            value: VIRTIO_MMIO_VERSION_VALUE,
            address_reg: HV_REG_X2,
            instruction: AARCH64_LDR_W0_FROM_X2,
        },
        BlockIdentityRegisterSpec {
            name: "device_id",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DEVICE_ID_OFFSET,
            value: VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE,
            address_reg: HV_REG_X3,
            instruction: AARCH64_LDR_W0_FROM_X3,
        },
        BlockIdentityRegisterSpec {
            name: "vendor_id",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_VENDOR_ID_OFFSET,
            value: VIRTIO_MMIO_VENDOR_ID_VALUE,
            address_reg: HV_REG_X4,
            instruction: AARCH64_LDR_W0_FROM_X4,
        },
    ]
}

pub(crate) fn block_register_probe_defaults() -> Vec<HvfMmioBlockRegisterProbe> {
    block_identity_register_specs()
        .iter()
        .map(|spec| HvfMmioBlockRegisterProbe {
            name: spec.name,
            ipa: spec.ipa,
            expected_value: spec.value,
            run_attempted: false,
            exit_observed: false,
            handled_by_device: false,
            value_injected: false,
            pc_read_after_exit: false,
            pc_advanced: false,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            watchdog_cancel_status: None,
            value_set_status: None,
            pc_read_status: None,
            pc_after_exit: None,
            pc_advance_status: None,
        })
        .collect()
}

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

pub(crate) fn block_queue_step_specs() -> [BlockQueueStepSpec; 26] {
    [
        BlockQueueStepSpec {
            name: "device_features",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "driver_features",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DRIVER_FEATURES_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_ack",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_driver",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_features_ok",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_select",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_SEL_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_num_max",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_num",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_desc_low",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_desc_high",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_driver_low",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_driver_high",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_device_low",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_device_high",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_ready",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_READY_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_driver_ok",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_notify",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_ready",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_READY_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_desc_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_driver_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_device_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "interrupt_status",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            expected_value: Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "config_generation",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_CONFIG_GENERATION_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "capacity_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "capacity_high",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
    ]
}

pub(crate) fn block_queue_step_defaults() -> Vec<HvfMmioBlockQueueStepProbe> {
    block_queue_step_specs()
        .iter()
        .map(|spec| HvfMmioBlockQueueStepProbe {
            name: spec.name,
            access: spec.access.as_str(),
            ipa: spec.ipa,
            expected_value: spec.expected_value,
            write_value: spec.write_value,
            run_attempted: false,
            address_register_set: false,
            write_value_register_set: false,
            exit_observed: false,
            handled_by_device: false,
            value_injected: false,
            write_accepted: false,
            pc_read_after_exit: false,
            pc_advanced: false,
            captured_write_value: None,
            run_status: None,
            address_register_set_status: None,
            write_value_register_set_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            watchdog_cancel_status: None,
            value_set_status: None,
            pc_read_status: None,
            pc_after_exit: None,
            pc_advance_status: None,
        })
        .collect()
}

pub fn probe_hvf_mmio_block_queue(
    allow_device: bool,
    disk_path: Option<PathBuf>,
    iso_path: Option<PathBuf>,
    writable_disk_path: Option<PathBuf>,
    host: HvfHostCapabilities,
) -> HvfMmioBlockQueueProbe {
    let mut blockers = Vec::new();
    let block_backing = if let Some(path) = writable_disk_path.as_ref() {
        VirtioBlockProbeBackingRef::HostFileWritable(path)
    } else if let Some(path) = iso_path.as_ref() {
        VirtioBlockProbeBackingRef::HostIsoReadOnly(path)
    } else if let Some(path) = disk_path.as_ref() {
        VirtioBlockProbeBackingRef::HostFile(path)
    } else {
        VirtioBlockProbeBackingRef::Synthetic
    };
    let block_backing_kind = block_backing.kind();
    let block_backing_path = block_backing.path().cloned();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE=1 or pass --allow-device to emulate VirtIO-MMIO block queue/config/address/notify registers through the MMIO bus".to_string(),
        );
        return mmio_block_queue_probe_result(
            false,
            false,
            host,
            block_backing_kind,
            block_backing_path,
            blockers,
        );
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_block_queue_probe_result(
            true,
            false,
            host,
            block_backing_kind,
            block_backing_path,
            blockers,
        );
    }

    let specs = block_queue_step_specs();
    let mut steps = block_queue_step_defaults();
    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut continuation_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut capacity_high_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut continuation_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut continuation_watchdog_cancel_status = None;
    let mut capacity_high_after_continue_status = None;
    let mut capacity_high_after_continue = None;
    let mut request_ring_seeded = false;
    let mut request_completed_after_notify = false;
    let mut request_descriptor_index = None;
    let mut request_sector = None;
    let mut request_byte_offset = None;
    let mut request_data_bytes = None;
    let mut request_data_prefix = Vec::new();
    let mut request_status = None;
    let mut request_used_index = None;
    let mut request_used_len = None;
    let mut request_interrupt_status = None;
    let mut write_completed_after_notify = false;
    let mut write_request_type = None;
    let mut write_sector = None;
    let mut write_byte_offset = None;
    let mut write_data_bytes = None;
    let mut write_data_prefix = Vec::new();
    let mut write_status = None;
    let mut write_used_index = None;
    let mut write_used_len = None;
    let mut flush_completed_after_notify = false;
    let mut flush_request_type = None;
    let mut flush_status = None;
    let mut flush_used_index = None;
    let mut flush_used_len = None;
    let mut persisted_data_prefix = Vec::new();
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
            let seed_result: Result<(), VirtioBlockRequestError> = unsafe {
                let bytes = std::slice::from_raw_parts_mut(memory.cast::<u8>(), PROBE_BYTES);
                let mut guest_memory = VirtioGuestMemory::new(PROBE_IPA_START, bytes);
                seed_synthetic_virtio_block_read_request(&mut guest_memory)
            };
            match seed_result {
                Ok(()) => request_ring_seeded = true,
                Err(error) => blockers.push(format!(
                    "failed to seed synthetic VirtIO block request ring: {}",
                    error.render_blocker()
                )),
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

    let mut can_continue = vcpu_created && pc_set && cpsr_set;
    for (index, spec) in specs.iter().enumerate() {
        if !can_continue {
            break;
        }

        let step = &mut steps[index];
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, spec.ipa) };
        step.address_register_set_status = Some(status);
        step.address_register_set = status == HV_SUCCESS;
        if !step.address_register_set {
            blockers.push(format!(
                "hv_vcpu_set_reg(X1 {}) failed: {status:#x}",
                spec.name
            ));
            break;
        }

        if let Some(value) = spec.write_value {
            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
            step.write_value_register_set_status = Some(status);
            step.write_value_register_set = status == HV_SUCCESS;
            if !step.write_value_register_set {
                blockers.push(format!(
                    "hv_vcpu_set_reg(X0 {}) failed: {status:#x}",
                    spec.name
                ));
                break;
            }
        }

        step.run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        step.run_status = Some(observation.run_status);
        step.exit_reason = observation.exit_reason;
        step.exit_syndrome = observation.exit_syndrome;
        step.exit_virtual_address = observation.exit_virtual_address;
        step.exit_physical_address = observation.exit_physical_address;
        step.watchdog_cancel_status = observation.watchdog_cancel_status;
        if step.watchdog_cancel_status.is_some() {
            blockers.push(format!(
                "VirtIO block queue/config {} {} watchdog fired before exception exit",
                spec.access.as_str(),
                spec.name
            ));
            can_continue = false;
        }

        if observation.run_status == HV_SUCCESS {
            if step.exit_reason.is_none() {
                blockers.push(format!(
                    "VirtIO block queue/config {} {} returned success without an exit info pointer",
                    spec.access.as_str(),
                    spec.name
                ));
                can_continue = false;
            } else {
                step.exit_observed = step.exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (step.exit_virtual_address == Some(spec.ipa)
                        || step.exit_physical_address == Some(spec.ipa)
                        || step.exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !step.exit_observed {
                    blockers.push(format!(
                        "VirtIO block queue/config {} {} did not exit at IPA {:#x}",
                        spec.access.as_str(),
                        spec.name,
                        spec.ipa
                    ));
                    can_continue = false;
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block queue/config {} {} hv_vcpu_run failed: {:#x}",
                spec.access.as_str(),
                spec.name,
                observation.run_status
            ));
            can_continue = false;
        }

        if step.exit_observed {
            match spec.access {
                BlockQueueAccessKind::Read => {
                    match mmio_bus.dispatch(MmioAccess::read(spec.ipa, 4)) {
                        MmioAction::ReadValue(value) => {
                            step.handled_by_device = true;
                            if Some(value) != spec.expected_value {
                                blockers.push(format!(
                                    "VirtIO block queue/config {} read returned unexpected value: {value:#x}",
                                    spec.name
                                ));
                                can_continue = false;
                            }
                            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                            step.value_set_status = Some(status);
                            step.value_injected = status == HV_SUCCESS;
                            if !step.value_injected {
                                blockers.push(format!(
                                    "hv_vcpu_set_reg(X0 read {}) failed: {status:#x}",
                                    spec.name
                                ));
                                can_continue = false;
                            }
                        }
                        MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                            blockers.push(format!(
                                "VirtIO block queue/config {} read was not handled by the MMIO device bus at {:#x}",
                                spec.name, spec.ipa
                            ));
                            can_continue = false;
                        }
                    }
                }
                BlockQueueAccessKind::Write => {
                    let value = spec.write_value.expect("write step has a seed value");
                    match mmio_bus.dispatch(MmioAccess::write(spec.ipa, value, 4)) {
                        MmioAction::WriteAccepted {
                            value: accepted_value,
                            ..
                        } => {
                            step.handled_by_device = true;
                            step.captured_write_value = Some(accepted_value);
                            step.write_accepted = accepted_value == value;
                            if !step.write_accepted {
                                blockers.push(format!(
                                    "VirtIO block queue/config {} write accepted unexpected value: {accepted_value:#x}",
                                    spec.name
                                ));
                                can_continue = false;
                            }
                            if spec.name == "queue_notify"
                                && step.write_accepted
                                && request_ring_seeded
                            {
                                match mmio_bus.find_device_mut::<VirtioMmioBlockDevice>() {
                                    Some(block) => {
                                        let completion_result: Result<
                                            VirtioBlockQueueProbeCompletion,
                                            VirtioBlockRequestError,
                                        > = unsafe {
                                            let bytes = std::slice::from_raw_parts_mut(
                                                memory.cast::<u8>(),
                                                PROBE_BYTES,
                                            );
                                            let mut guest_memory =
                                                VirtioGuestMemory::new(PROBE_IPA_START, bytes);
                                            match block_backing {
                                                VirtioBlockProbeBackingRef::HostFileWritable(
                                                    path,
                                                ) => {
                                                    complete_probe_virtio_block_writable_file_requests(
                                                        block,
                                                        &mut guest_memory,
                                                        path,
                                                    )
                                                    .map(VirtioBlockQueueProbeCompletion::Writable)
                                                }
                                                _ => complete_probe_virtio_block_request(
                                                    block,
                                                    &mut guest_memory,
                                                    block_backing,
                                                )
                                                .map(VirtioBlockQueueProbeCompletion::ReadOnly),
                                            }
                                        };
                                        match completion_result {
                                            Ok(VirtioBlockQueueProbeCompletion::ReadOnly(
                                                probe_completion,
                                            )) => {
                                                request_completed_after_notify = true;
                                                request_descriptor_index = Some(
                                                    probe_completion.completion.descriptor_index,
                                                );
                                                request_sector =
                                                    Some(probe_completion.completion.sector);
                                                request_byte_offset =
                                                    Some(probe_completion.byte_offset);
                                                request_data_bytes =
                                                    Some(probe_completion.completion.data_bytes);
                                                request_data_prefix = probe_completion.data_prefix;
                                                request_status = Some(probe_completion.status);
                                                request_used_index =
                                                    Some(probe_completion.completion.used_index);
                                                request_used_len = Some(probe_completion.used_len);
                                                request_interrupt_status = Some(
                                                    probe_completion.completion.interrupt_status,
                                                );
                                            }
                                            Ok(VirtioBlockQueueProbeCompletion::Writable(
                                                probe_completion,
                                            )) => {
                                                let initial_read = probe_completion.initial_read;
                                                request_completed_after_notify = true;
                                                request_descriptor_index =
                                                    Some(initial_read.completion.descriptor_index);
                                                request_sector =
                                                    Some(initial_read.completion.sector);
                                                request_byte_offset =
                                                    Some(initial_read.byte_offset);
                                                request_data_bytes =
                                                    Some(initial_read.completion.data_bytes);
                                                request_data_prefix = initial_read.data_prefix;
                                                request_status = Some(initial_read.status);
                                                request_used_index =
                                                    Some(initial_read.completion.used_index);
                                                request_used_len = Some(initial_read.used_len);
                                                request_interrupt_status =
                                                    Some(initial_read.completion.interrupt_status);

                                                write_completed_after_notify = true;
                                                write_request_type = Some(
                                                    probe_completion.write_completion.request_type,
                                                );
                                                write_sector =
                                                    Some(probe_completion.write_completion.sector);
                                                write_byte_offset =
                                                    Some(probe_completion.write_byte_offset);
                                                write_data_bytes = Some(
                                                    probe_completion.write_completion.data_bytes,
                                                );
                                                write_data_prefix =
                                                    probe_completion.write_data_prefix;
                                                write_status = Some(probe_completion.write_status);
                                                write_used_index = Some(
                                                    probe_completion.write_completion.used_index,
                                                );
                                                write_used_len =
                                                    Some(probe_completion.write_used_len);

                                                flush_completed_after_notify = true;
                                                flush_request_type = Some(
                                                    probe_completion.flush_completion.request_type,
                                                );
                                                flush_status = Some(probe_completion.flush_status);
                                                flush_used_index = Some(
                                                    probe_completion.flush_completion.used_index,
                                                );
                                                flush_used_len =
                                                    Some(probe_completion.flush_used_len);
                                                persisted_data_prefix =
                                                    probe_completion.persisted_data_prefix;
                                            }
                                            Err(error) => {
                                                blockers.push(format!(
                                                    "VirtIO block request completion after queue_notify failed: {}",
                                                    error.render_blocker()
                                                ));
                                                can_continue = false;
                                            }
                                        }
                                    }
                                    None => {
                                        blockers.push(
                                            "VirtIO block request completion after queue_notify could not find the block device on the MMIO bus"
                                                .to_string(),
                                        );
                                        can_continue = false;
                                    }
                                }
                            } else if spec.name == "queue_notify" && step.write_accepted {
                                blockers.push(
                                    "VirtIO block request ring was not seeded before queue_notify"
                                        .to_string(),
                                );
                                can_continue = false;
                            }
                        }
                        MmioAction::ReadValue(_) | MmioAction::Unhandled => {
                            blockers.push(format!(
                                "VirtIO block queue/config {} write was not handled by the MMIO device bus at {:#x}",
                                spec.name, spec.ipa
                            ));
                            can_continue = false;
                        }
                    }
                }
            }
        }

        if step.exit_observed {
            let mut pc = 0;
            let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
            step.pc_read_status = Some(status);
            step.pc_read_after_exit = status == HV_SUCCESS;
            if step.pc_read_after_exit {
                step.pc_after_exit = Some(pc);
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_reg(PC after VirtIO block queue/config {} {}) failed: {status:#x}",
                    spec.access.as_str(),
                    spec.name
                ));
                can_continue = false;
            }
        }

        let step_completed = match spec.access {
            BlockQueueAccessKind::Read => step.value_injected,
            BlockQueueAccessKind::Write => step.write_accepted,
        };
        if step.pc_read_after_exit && step_completed {
            let next_pc = PROBE_IPA_START + ((index as u64 + 1) * 4);
            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, next_pc) };
            step.pc_advance_status = Some(status);
            step.pc_advanced = status == HV_SUCCESS;
            if !step.pc_advanced {
                blockers.push(format!(
                    "hv_vcpu_set_reg(PC after VirtIO block queue/config {} {}) failed: {status:#x}",
                    spec.access.as_str(),
                    spec.name
                ));
                can_continue = false;
            }
        }
    }

    if steps.iter().all(|step| step.pc_advanced) {
        continuation_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        continuation_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        continuation_watchdog_cancel_status = observation.watchdog_cancel_status;
        if continuation_watchdog_cancel_status.is_some() {
            blockers.push(
                "VirtIO block queue/config continuation watchdog fired before HVC exit".to_string(),
            );
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "VirtIO block queue/config continuation returned success without an exit info pointer"
                        .to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "VirtIO block queue/config continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| format!("{value:#x}")
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block queue/config continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        capacity_high_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            capacity_high_after_continue = Some(value);
            capacity_high_value_preserved = value == (VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32);
            if !capacity_high_value_preserved {
                blockers.push(format!(
                    "VirtIO block capacity high value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after VirtIO block queue/config continue) failed: {status:#x}"
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

    let watchdog_cancel_fired = steps
        .iter()
        .any(|step| step.watchdog_cancel_status.is_some())
        || continuation_watchdog_cancel_status.is_some();

    HvfMmioBlockQueueProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        device_bus_created,
        device_bus_device_count,
        steps,
        continuation_run_attempted,
        continuation_exit_observed,
        capacity_high_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_models: BLOCK_QUEUE_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0",
        device_features_value: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
        driver_features_value: VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
        queue_select_value: VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
        queue_num_max_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
        queue_num_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        queue_ready_value: VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        queue_desc_address: VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS,
        queue_driver_address: VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS,
        queue_device_address: VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS,
        queue_notify_value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        interrupt_status_value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
        block_backing_kind,
        block_backing_path,
        request_ring_seeded,
        request_completed_after_notify,
        request_descriptor_index,
        request_sector,
        request_byte_offset,
        request_data_bytes,
        request_data_prefix,
        request_status,
        request_used_index,
        request_used_len,
        request_interrupt_status,
        write_completed_after_notify,
        write_request_type,
        write_sector,
        write_byte_offset,
        write_data_bytes,
        write_data_prefix,
        write_status,
        write_used_index,
        write_used_len,
        flush_completed_after_notify,
        flush_request_type,
        flush_status,
        flush_used_index,
        flush_used_len,
        persisted_data_prefix,
        status_value: VIRTIO_MMIO_BLOCK_STATUS_VALUE,
        capacity_sectors: VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        continuation_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        continuation_watchdog_cancel_status,
        capacity_high_after_continue_status,
        capacity_high_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn mmio_block_queue_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    block_backing_kind: &'static str,
    block_backing_path: Option<PathBuf>,
    blockers: Vec<String>,
) -> HvfMmioBlockQueueProbe {
    HvfMmioBlockQueueProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        steps: block_queue_step_defaults(),
        continuation_run_attempted: false,
        continuation_exit_observed: false,
        capacity_high_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_models: BLOCK_QUEUE_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0",
        device_features_value: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
        driver_features_value: VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
        queue_select_value: VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
        queue_num_max_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
        queue_num_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        queue_ready_value: VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        queue_desc_address: VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS,
        queue_driver_address: VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS,
        queue_device_address: VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS,
        queue_notify_value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        interrupt_status_value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
        block_backing_kind,
        block_backing_path,
        request_ring_seeded: false,
        request_completed_after_notify: false,
        request_descriptor_index: None,
        request_sector: None,
        request_byte_offset: None,
        request_data_bytes: None,
        request_data_prefix: Vec::new(),
        request_status: None,
        request_used_index: None,
        request_used_len: None,
        request_interrupt_status: None,
        write_completed_after_notify: false,
        write_request_type: None,
        write_sector: None,
        write_byte_offset: None,
        write_data_bytes: None,
        write_data_prefix: Vec::new(),
        write_status: None,
        write_used_index: None,
        write_used_len: None,
        flush_completed_after_notify: false,
        flush_request_type: None,
        flush_status: None,
        flush_used_index: None,
        flush_used_len: None,
        persisted_data_prefix: Vec::new(),
        status_value: VIRTIO_MMIO_BLOCK_STATUS_VALUE,
        capacity_sectors: VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        continuation_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        continuation_watchdog_cancel_status: None,
        capacity_high_after_continue_status: None,
        capacity_high_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub(crate) fn mmio_block_device_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioBlockDeviceProbe {
    HvfMmioBlockDeviceProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        register_address_registers_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        register_reads: block_register_probe_defaults(),
        continuation_run_attempted: false,
        continuation_exit_observed: false,
        vendor_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
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
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        register_address_registers_set_status: vec![None; 4],
        continuation_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        continuation_watchdog_cancel_status: None,
        vendor_value_after_continue_status: None,
        vendor_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}
