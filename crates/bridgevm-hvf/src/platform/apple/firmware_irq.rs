//! Firmware IRQ and virtual-timer delivery into the guest.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) fn firmware_vtimer_deadline(offset: u64) -> u64 {
    unsafe { mach_absolute_time() }
        .saturating_sub(offset)
        .saturating_add(WINDOWS_ARM_FIRMWARE_VTIMER_DEADLINE_TICKS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowsArmFirmwareIrqLineDelivery {
    pub(crate) irq_line_snapshot: GicV3CpuInterfaceIrqLineSnapshot,
    pub(crate) irq_line_should_assert: bool,
    pub(crate) pending_irq_status: Option<HvReturn>,
    pub(crate) next_device_irq_line_asserted: bool,
    pub(crate) device_irq_injected: bool,
    pub(crate) device_irq_cleared: bool,
}

impl WindowsArmFirmwareIrqLineDelivery {
    pub(crate) fn succeeded(self) -> bool {
        match self.pending_irq_status {
            Some(status) => status == HV_SUCCESS,
            None => true,
        }
    }

    pub(crate) fn failure_blocker(self, exit_index: u32) -> String {
        format!(
            "firmware run-loop GIC CPU-interface IRQ line refresh failed on exit {exit_index}: desired_pending={}, gic_group1_enabled={}, gic_priority_mask={:#x}, gic_running_priority={:#x}, gic_priority_threshold={:#x}, gic_pending_intid={}, hv_vcpu_set_pending_interrupt={:#x}",
            self.irq_line_should_assert,
            self.irq_line_snapshot.group1_enabled,
            self.irq_line_snapshot.priority_mask,
            self.irq_line_snapshot.running_priority,
            self.irq_line_snapshot.priority_threshold,
            self.irq_line_snapshot.pending_intid,
            self.pending_irq_status.unwrap_or(HV_SUCCESS)
        )
    }
}

pub(crate) fn service_windows_arm_firmware_gic_irq_line_delivery(
    vcpu: HvVcpu,
    bus: &mut MmioBus,
    block_devices: &[crate::WindowsArmVirtioBlockDeviceMetadata],
    gic_cpu_interface: &GicV3CpuInterfaceState,
    device_irq_line_asserted: bool,
    refresh_level_sources: bool,
) -> WindowsArmFirmwareIrqLineDelivery {
    if refresh_level_sources {
        let _ = refresh_windows_arm_firmware_device_irq_pending(bus, block_devices);
    }

    let irq_line_snapshot = gic_cpu_interface.irq_line_snapshot(bus);
    let irq_line_should_assert = irq_line_snapshot.irq_line_should_assert;
    let mut next_device_irq_line_asserted = device_irq_line_asserted;
    let mut device_irq_injected = false;
    let mut device_irq_cleared = false;
    let pending_irq_status = if irq_line_should_assert != device_irq_line_asserted {
        let status = unsafe {
            hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, irq_line_should_assert)
        };
        if status == HV_SUCCESS {
            next_device_irq_line_asserted = irq_line_should_assert;
            if irq_line_should_assert {
                device_irq_injected = true;
            } else {
                device_irq_cleared = true;
            }
        }
        Some(status)
    } else {
        None
    };

    WindowsArmFirmwareIrqLineDelivery {
        irq_line_snapshot,
        irq_line_should_assert,
        pending_irq_status,
        next_device_irq_line_asserted,
        device_irq_injected,
        device_irq_cleared,
    }
}

pub(crate) fn record_windows_arm_firmware_irq_line_delivery(
    delivery: WindowsArmFirmwareIrqLineDelivery,
    device_irq_line_asserted: &mut bool,
    last_device_irq_set_status: &mut Option<HvReturn>,
    last_device_irq_clear_status: &mut Option<HvReturn>,
    device_irq_injected_count: &mut u32,
    device_irq_cleared_count: &mut u32,
) {
    if let Some(status) = delivery.pending_irq_status {
        if delivery.irq_line_should_assert {
            *last_device_irq_set_status = Some(status);
        } else {
            *last_device_irq_clear_status = Some(status);
        }
    }
    if delivery.device_irq_injected {
        *device_irq_injected_count += 1;
    }
    if delivery.device_irq_cleared {
        *device_irq_cleared_count += 1;
    }
    *device_irq_line_asserted = delivery.next_device_irq_line_asserted;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowsArmFirmwareVtimerDelivery {
    pub(crate) rearm_cval_value: u64,
    pub(crate) rearm_cval_status: HvReturn,
    pub(crate) ppi_pending_recorded: bool,
    pub(crate) irq_line_snapshot: GicV3CpuInterfaceIrqLineSnapshot,
    pub(crate) irq_line_should_assert: bool,
    pub(crate) pending_irq_status: Option<HvReturn>,
    pub(crate) unmask_status: Option<HvReturn>,
    pub(crate) next_device_irq_line_asserted: bool,
    pub(crate) device_irq_injected: bool,
    pub(crate) device_irq_cleared: bool,
}

impl WindowsArmFirmwareVtimerDelivery {
    pub(crate) fn irq_effective_status(self) -> HvReturn {
        self.pending_irq_status.unwrap_or(HV_SUCCESS)
    }

    pub(crate) fn pending_irq_injected(self) -> bool {
        self.irq_line_should_assert
    }

    pub(crate) fn succeeded(self) -> bool {
        self.rearm_cval_status == HV_SUCCESS
            && self.ppi_pending_recorded
            && self.irq_effective_status() == HV_SUCCESS
            && self.unmask_status.unwrap_or(HV_SUCCESS) == HV_SUCCESS
    }

    pub(crate) fn failure_blocker(self, exit_index: u32) -> String {
        format!(
            "firmware run-loop failed to service VTimer exit {exit_index}: hv_vcpu_set_sys_reg(CNTV_CVAL_EL0)={:#x}, timer_ppi_pending_recorded={}, gic_group1_enabled={}, gic_priority_mask={:#x}, gic_running_priority={:#x}, gic_priority_threshold={:#x}, gic_pending_intid={}, hv_vcpu_set_pending_interrupt(IRQ={})={:#x}, hv_vcpu_set_vtimer_mask(false)={}",
            self.rearm_cval_status,
            self.ppi_pending_recorded,
            self.irq_line_snapshot.group1_enabled,
            self.irq_line_snapshot.priority_mask,
            self.irq_line_snapshot.running_priority,
            self.irq_line_snapshot.priority_threshold,
            self.irq_line_snapshot.pending_intid,
            self.irq_line_should_assert,
            self.irq_effective_status(),
            crate::render_optional_status(self.unmask_status)
        )
    }
}

pub(crate) fn service_windows_arm_firmware_vtimer_delivery(
    vcpu: HvVcpu,
    bus: &mut MmioBus,
    gic_cpu_interface: &GicV3CpuInterfaceState,
    device_irq_line_asserted: bool,
    unmask_without_assertable_irq: bool,
) -> WindowsArmFirmwareVtimerDelivery {
    let rearm_cval_value = firmware_vtimer_deadline(WINDOWS_ARM_VTIMER_OFFSET_VALUE);
    let rearm_cval_status =
        unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, rearm_cval_value) };
    let ppi_pending_recorded = set_windows_arm_firmware_vtimer_ppi_pending(bus, true);
    let irq_line_snapshot = gic_cpu_interface.irq_line_snapshot(bus);
    let irq_line_should_assert = irq_line_snapshot.irq_line_should_assert;

    let mut next_device_irq_line_asserted = device_irq_line_asserted;
    let mut device_irq_injected = false;
    let device_irq_cleared = false;
    let pending_irq_status =
        if irq_line_should_assert && irq_line_should_assert != device_irq_line_asserted {
            let status = unsafe {
                hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, irq_line_should_assert)
            };
            if status == HV_SUCCESS {
                next_device_irq_line_asserted = true;
                device_irq_injected = true;
            }
            Some(status)
        } else {
            None
        };
    let unmask_status = if irq_line_should_assert || unmask_without_assertable_irq {
        let status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        Some(status)
    } else {
        None
    };

    WindowsArmFirmwareVtimerDelivery {
        rearm_cval_value,
        rearm_cval_status,
        ppi_pending_recorded,
        irq_line_snapshot,
        irq_line_should_assert,
        pending_irq_status,
        unmask_status,
        next_device_irq_line_asserted,
        device_irq_injected,
        device_irq_cleared,
    }
}
