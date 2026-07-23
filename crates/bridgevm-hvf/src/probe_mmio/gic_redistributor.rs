//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GicV3RedistributorDevice {
    pub(crate) base_ipa: u64,
    pub(crate) ctlr: u32,
    pub(crate) waker: u32,
    pub(crate) propbaser: u64,
    pub(crate) pendbaser: u64,
    pub(crate) group0: u32,
    pub(crate) group_modifier0: u32,
    pub(crate) enabled0: u32,
    pub(crate) pending0: u32,
    pub(crate) active0: u32,
    pub(crate) priority: [u32; 8],
    pub(crate) config0: u32,
}

impl GicV3RedistributorDevice {
    pub(crate) fn new(base_ipa: u64) -> Self {
        Self {
            base_ipa,
            ctlr: 0,
            waker: 0,
            propbaser: 0,
            pendbaser: 0,
            group0: 0,
            group_modifier0: 0,
            enabled0: 0,
            pending0: 0,
            active0: 0,
            priority: [GICV3_DEFAULT_PRIORITY_WORD; 8],
            config0: 0,
        }
    }

    pub(crate) fn write_waker(&mut self, value: u64, width: u8) -> MmioAction {
        let value = mask_mmio_value(value, width) as u32;
        if value & GICR_WAKER_PROCESSOR_SLEEP as u32 != 0 {
            self.waker = (GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP) as u32;
        } else {
            self.waker = 0;
        }
        MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        }
    }

    pub(crate) fn fdt_ppi_interrupt_id(ppi: u32) -> Option<u32> {
        let interrupt_id = 16_u32.checked_add(ppi)?;
        (interrupt_id < 32).then_some(interrupt_id)
    }

    pub(crate) fn interrupt_priority(&self, interrupt_id: u32) -> Option<u8> {
        if interrupt_id >= 32 {
            return None;
        }
        let register = usize::try_from(interrupt_id / 4).ok()?;
        let shift = (interrupt_id % 4) * 8;
        Some(((self.priority[register] >> shift) & 0xff) as u8)
    }

    pub(crate) fn set_fdt_ppi_pending(&mut self, ppi: u32, pending: bool) -> bool {
        let Some(interrupt_id) = Self::fdt_ppi_interrupt_id(ppi) else {
            return false;
        };
        let bit = 1_u32 << interrupt_id;
        if pending {
            self.pending0 |= bit;
        } else {
            self.pending0 &= !bit;
        }
        true
    }

    pub(crate) fn pending_interrupt_for_cpu(
        &self,
        priority_mask: u8,
    ) -> Option<GicV3PendingInterrupt> {
        if self.waker & GICR_WAKER_PROCESSOR_SLEEP as u32 != 0 {
            return None;
        }
        (16_u32..32)
            .filter_map(|interrupt_id| {
                let bit = 1_u32 << interrupt_id;
                let group1 = (self.group0 & bit) != 0;
                let enabled = (self.enabled0 & bit) != 0;
                let pending = (self.pending0 & bit) != 0;
                let active = (self.active0 & bit) != 0;
                let priority = self.interrupt_priority(interrupt_id)?;
                (group1 && enabled && pending && !active && priority < priority_mask).then_some(
                    GicV3PendingInterrupt {
                        interrupt_id,
                        priority,
                    },
                )
            })
            .min_by_key(|interrupt| (interrupt.priority, interrupt.interrupt_id))
    }

    #[cfg(test)]
    pub(crate) fn pending_interrupt_id_for_cpu(&self, priority_mask: u8) -> Option<u32> {
        self.pending_interrupt_for_cpu(priority_mask)
            .map(|interrupt| interrupt.interrupt_id)
    }

    pub(crate) fn acknowledge_interrupt_id(&mut self, interrupt_id: u32) -> bool {
        if interrupt_id >= 32 {
            return false;
        }
        let bit = 1_u32 << interrupt_id;
        let was_pending = (self.pending0 & bit) != 0;
        if was_pending {
            self.pending0 &= !bit;
            self.active0 |= bit;
        }
        was_pending
    }

    #[cfg(test)]
    pub(crate) fn acknowledge_pending_interrupt(&mut self, priority_mask: u8) -> u32 {
        let Some(interrupt) = self.pending_interrupt_for_cpu(priority_mask) else {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        };
        if !self.acknowledge_interrupt_id(interrupt.interrupt_id) {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        }
        interrupt.interrupt_id
    }

    pub(crate) fn end_interrupt(&mut self, interrupt_id: u32) -> bool {
        if interrupt_id >= 32 {
            return false;
        }
        let bit = 1_u32 << interrupt_id;
        let was_active = (self.active0 & bit) != 0;
        self.active0 &= !bit;
        was_active
    }
}

impl MmioDevice for GicV3RedistributorDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        let offset = access.ipa.saturating_sub(self.base_ipa);
        match (access.kind, offset, access.value) {
            (MmioAccessKind::Read, GICR_CTLR_OFFSET, None) => {
                MmioAction::ReadValue(u64::from(self.ctlr))
            }
            (MmioAccessKind::Read, GICR_IIDR_OFFSET, None) => {
                MmioAction::ReadValue(GICV3_IIDR_VALUE)
            }
            (MmioAccessKind::Read, current, None) => {
                if let Some(value) = GicV3DistributorDevice::read_u64_register(
                    current,
                    GICR_TYPER_OFFSET,
                    GICR_TYPER_VALUE,
                    access.width,
                ) {
                    return MmioAction::ReadValue(value);
                }
                match current {
                    GICR_STATUSR_OFFSET => MmioAction::ReadValue(0),
                    GICR_WAKER_OFFSET => MmioAction::ReadValue(u64::from(self.waker)),
                    GICR_SGI_IGROUPR0_OFFSET => MmioAction::ReadValue(u64::from(self.group0)),
                    GICR_SGI_ISENABLER0_OFFSET | GICR_SGI_ICENABLER0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.enabled0))
                    }
                    GICR_SGI_ISPENDR0_OFFSET | GICR_SGI_ICPENDR0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.pending0))
                    }
                    GICR_SGI_ISACTIVER0_OFFSET | GICR_SGI_ICACTIVER0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.active0))
                    }
                    GICR_SGI_ICFGR0_OFFSET => MmioAction::ReadValue(u64::from(self.config0)),
                    GICR_SGI_IGRPMODR0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.group_modifier0))
                    }
                    _ => {
                        if let Some(value) = GicV3DistributorDevice::read_u64_register(
                            current,
                            GICR_PROPBASER_OFFSET,
                            self.propbaser,
                            access.width,
                        ) {
                            return MmioAction::ReadValue(value);
                        }
                        if let Some(value) = GicV3DistributorDevice::read_u64_register(
                            current,
                            GICR_PENDBASER_OFFSET,
                            self.pendbaser,
                            access.width,
                        ) {
                            return MmioAction::ReadValue(value);
                        }
                        if let Some(action) = GicV3DistributorDevice::read_byte_indexed_register(
                            current,
                            GICR_SGI_IPRIORITYR_BASE_OFFSET,
                            &self.priority,
                            access.width,
                        ) {
                            return action;
                        }
                        MmioAction::Unhandled
                    }
                }
            }
            (MmioAccessKind::Write, GICR_CTLR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width) as u32;
                self.ctlr = value;
                MmioAction::WriteAccepted {
                    value: u64::from(value),
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, GICR_WAKER_OFFSET, Some(value)) => {
                self.write_waker(value, access.width)
            }
            (MmioAccessKind::Write, GICR_STATUSR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, current, Some(value)) => match current {
                GICR_SGI_IGROUPR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.group0 = value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ISENABLER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.enabled0 |= value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICENABLER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.enabled0 &= !value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ISPENDR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.pending0 |= value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICPENDR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.pending0 &= !value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ISACTIVER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.active0 |= value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICACTIVER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.active0 &= !value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICFGR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.config0 = value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_IGRPMODR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.group_modifier0 = value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                _ => {
                    if let Some(propbaser) = GicV3DistributorDevice::write_u64_register(
                        self.propbaser,
                        current,
                        GICR_PROPBASER_OFFSET,
                        value,
                        access.width,
                    ) {
                        self.propbaser = propbaser;
                        let value = mask_mmio_value(value, access.width);
                        return MmioAction::WriteAccepted {
                            value,
                            byte: (value & 0xff) as u8,
                        };
                    }
                    if let Some(pendbaser) = GicV3DistributorDevice::write_u64_register(
                        self.pendbaser,
                        current,
                        GICR_PENDBASER_OFFSET,
                        value,
                        access.width,
                    ) {
                        self.pendbaser = pendbaser;
                        let value = mask_mmio_value(value, access.width);
                        return MmioAction::WriteAccepted {
                            value,
                            byte: (value & 0xff) as u8,
                        };
                    }
                    if let Some(action) = GicV3DistributorDevice::write_byte_indexed_register(
                        current,
                        GICR_SGI_IPRIORITYR_BASE_OFFSET,
                        &mut self.priority,
                        value,
                        access.width,
                    ) {
                        return action;
                    }
                    MmioAction::Unhandled
                }
            },
            _ => MmioAction::Unhandled,
        }
    }
}
