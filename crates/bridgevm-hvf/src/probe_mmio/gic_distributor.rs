//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GicV3DistributorDevice {
    pub(crate) base_ipa: u64,
    pub(crate) ctlr: u32,
    pub(crate) statusr: u32,
    pub(crate) group: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) group_modifier: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) enabled: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) pending: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) active: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) priority: [u32; GICV3_PRIORITY_REGISTER_COUNT],
    pub(crate) config: [u32; GICV3_CONFIG_REGISTER_COUNT],
    pub(crate) route: [u64; GICV3_SUPPORTED_INTERRUPT_COUNT],
}

impl GicV3DistributorDevice {
    pub(crate) fn new(base_ipa: u64) -> Self {
        Self {
            base_ipa,
            ctlr: 0,
            statusr: 0,
            group: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            group_modifier: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            enabled: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            pending: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            active: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            priority: [GICV3_DEFAULT_PRIORITY_WORD; GICV3_PRIORITY_REGISTER_COUNT],
            config: [0; GICV3_CONFIG_REGISTER_COUNT],
            route: [0; GICV3_SUPPORTED_INTERRUPT_COUNT],
        }
    }

    pub(crate) fn reg_index(offset: u64, base: u64, count: usize) -> Option<usize> {
        let end = base.checked_add((count as u64).checked_mul(4)?)?;
        if offset < base || offset >= end || (offset - base) % 4 != 0 {
            return None;
        }
        usize::try_from((offset - base) / 4).ok()
    }

    pub(crate) fn irouter_interrupt_id(offset: u64) -> Option<usize> {
        if !(GICD_IROUTER_BASE_OFFSET..GICD_IROUTER_BASE_OFFSET + 0x2000).contains(&offset) {
            return None;
        }
        let relative = offset - GICD_IROUTER_BASE_OFFSET;
        let interrupt_id = usize::try_from(relative / 8).ok()?;
        (interrupt_id < GICV3_SUPPORTED_INTERRUPT_COUNT).then_some(interrupt_id)
    }

    pub(crate) fn read_u64_register(offset: u64, base: u64, value: u64, width: u8) -> Option<u64> {
        match offset {
            current if current == base => Some(if width >= 8 {
                value
            } else {
                value & 0xffff_ffff
            }),
            current if current == base + 4 => Some(value >> 32),
            _ => None,
        }
    }

    pub(crate) fn write_u64_register(
        current: u64,
        offset: u64,
        base: u64,
        value: u64,
        width: u8,
    ) -> Option<u64> {
        let value = mask_mmio_value(value, width);
        match offset {
            current_offset if current_offset == base => Some(if width >= 8 {
                value
            } else {
                (current & 0xffff_ffff_0000_0000) | (value & 0xffff_ffff)
            }),
            current_offset if current_offset == base + 4 => {
                Some((current & 0x0000_0000_ffff_ffff) | ((value & 0xffff_ffff) << 32))
            }
            _ => None,
        }
    }

    pub(crate) fn read_indexed_register(
        offset: u64,
        base: u64,
        registers: &[u32],
    ) -> Option<MmioAction> {
        Self::reg_index(offset, base, registers.len())
            .map(|index| MmioAction::ReadValue(u64::from(registers[index])))
    }

    pub(crate) fn byte_register_access_offset(
        offset: u64,
        base: u64,
        registers: &[u32],
        width: u8,
    ) -> Option<usize> {
        let access_bytes = usize::from(width);
        if access_bytes == 0 || access_bytes > 8 {
            return None;
        }
        let end = base.checked_add((registers.len() as u64).checked_mul(4)?)?;
        let access_end = offset.checked_add(u64::from(width))?;
        if offset < base || access_end > end {
            return None;
        }
        usize::try_from(offset - base).ok()
    }

    pub(crate) fn read_byte_indexed_register(
        offset: u64,
        base: u64,
        registers: &[u32],
        width: u8,
    ) -> Option<MmioAction> {
        let byte_offset = Self::byte_register_access_offset(offset, base, registers, width)?;
        let mut value = 0_u64;
        for byte_index in 0..usize::from(width) {
            let absolute_byte = byte_offset + byte_index;
            let register = registers[absolute_byte / 4];
            let register_shift = (absolute_byte % 4) * 8;
            let byte = (register >> register_shift) & 0xff;
            value |= u64::from(byte) << (byte_index * 8);
        }
        Some(MmioAction::ReadValue(value))
    }

    pub(crate) fn write_byte_indexed_register(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let byte_offset = Self::byte_register_access_offset(offset, base, registers, width)?;
        let value = mask_mmio_value(value, width);
        for byte_index in 0..usize::from(width) {
            let absolute_byte = byte_offset + byte_index;
            let register = &mut registers[absolute_byte / 4];
            let register_shift = (absolute_byte % 4) * 8;
            let mask = 0xff_u32 << register_shift;
            let byte = ((value >> (byte_index * 8)) as u32 & 0xff) << register_shift;
            *register = (*register & !mask) | byte;
        }
        Some(MmioAction::WriteAccepted {
            value,
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn write_indexed_register(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let index = Self::reg_index(offset, base, registers.len())?;
        let value = mask_mmio_value(value, width) as u32;
        registers[index] = value;
        Some(MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn set_indexed_bits(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let index = Self::reg_index(offset, base, registers.len())?;
        let value = mask_mmio_value(value, width) as u32;
        registers[index] |= value;
        Some(MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn clear_indexed_bits(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let index = Self::reg_index(offset, base, registers.len())?;
        let value = mask_mmio_value(value, width) as u32;
        registers[index] &= !value;
        Some(MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn interrupt_bit(interrupt_id: usize) -> Option<(usize, u32)> {
        if interrupt_id >= GICV3_SUPPORTED_INTERRUPT_COUNT {
            return None;
        }
        Some((interrupt_id / 32, 1_u32 << (interrupt_id % 32)))
    }

    pub(crate) fn spi_interrupt_id(spi: u32) -> Option<usize> {
        let interrupt_id = 32_usize.checked_add(usize::try_from(spi).ok()?)?;
        (interrupt_id < GICV3_SUPPORTED_INTERRUPT_COUNT).then_some(interrupt_id)
    }

    pub(crate) fn set_spi_pending(&mut self, spi: u32, pending: bool) -> Option<()> {
        let interrupt_id = Self::spi_interrupt_id(spi)?;
        let (register, bit) = Self::interrupt_bit(interrupt_id)?;
        if pending {
            self.pending[register] |= bit;
        } else {
            self.pending[register] &= !bit;
        }
        Some(())
    }

    pub(crate) fn spi_irq_line_assertable(&self, spi: u32) -> bool {
        let Some(interrupt_id) = Self::spi_interrupt_id(spi) else {
            return false;
        };
        let Some((register, bit)) = Self::interrupt_bit(interrupt_id) else {
            return false;
        };
        (self.ctlr & GICD_CTLR_ENABLE_GRP1NS) != 0
            && (self.group[register] & bit) != 0
            && (self.enabled[register] & bit) != 0
            && (self.pending[register] & bit) != 0
            && (self.active[register] & bit) == 0
    }

    pub(crate) fn interrupt_priority(&self, interrupt_id: usize) -> Option<u8> {
        if interrupt_id >= GICV3_SUPPORTED_INTERRUPT_COUNT {
            return None;
        }
        let register = interrupt_id / 4;
        let shift = (interrupt_id % 4) * 8;
        Some(((self.priority[register] >> shift) & 0xff) as u8)
    }

    pub(crate) fn pending_interrupt_for_cpu(
        &self,
        priority_mask: u8,
    ) -> Option<GicV3PendingInterrupt> {
        if self.ctlr & GICD_CTLR_ENABLE_GRP1NS == 0 {
            return None;
        }
        (32..GICV3_SUPPORTED_INTERRUPT_COUNT)
            .filter_map(|interrupt_id| {
                let (register, bit) = Self::interrupt_bit(interrupt_id)?;
                let group1 = (self.group[register] & bit) != 0;
                let enabled = (self.enabled[register] & bit) != 0;
                let pending = (self.pending[register] & bit) != 0;
                let active = (self.active[register] & bit) != 0;
                let priority = self.interrupt_priority(interrupt_id)?;
                (group1 && enabled && pending && !active && priority < priority_mask).then_some(
                    GicV3PendingInterrupt {
                        interrupt_id: interrupt_id as u32,
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
        let Ok(interrupt_id) = usize::try_from(interrupt_id) else {
            return false;
        };
        let Some((register, bit)) = Self::interrupt_bit(interrupt_id) else {
            return false;
        };
        let was_pending = (self.pending[register] & bit) != 0;
        if was_pending {
            self.pending[register] &= !bit;
            self.active[register] |= bit;
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
        let Ok(interrupt_id) = usize::try_from(interrupt_id) else {
            return false;
        };
        let Some((register, bit)) = Self::interrupt_bit(interrupt_id) else {
            return false;
        };
        let was_active = (self.active[register] & bit) != 0;
        self.active[register] &= !bit;
        was_active
    }
}

impl MmioDevice for GicV3DistributorDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        let offset = access.ipa.saturating_sub(self.base_ipa);
        match (access.kind, offset, access.value) {
            (MmioAccessKind::Read, GICD_CTLR_OFFSET, None) => {
                MmioAction::ReadValue(u64::from(self.ctlr))
            }
            (MmioAccessKind::Read, GICD_TYPER_OFFSET, None) => {
                MmioAction::ReadValue(GICD_TYPER_VALUE)
            }
            (MmioAccessKind::Read, GICD_IIDR_OFFSET, None) => {
                MmioAction::ReadValue(GICV3_IIDR_VALUE)
            }
            (MmioAccessKind::Read, GICD_STATUSR_OFFSET, None) => {
                MmioAction::ReadValue(u64::from(self.statusr))
            }
            (MmioAccessKind::Read, current, None) => {
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_IGROUPR_BASE_OFFSET, &self.group)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ISENABLER_BASE_OFFSET, &self.enabled)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICENABLER_BASE_OFFSET, &self.enabled)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ISPENDR_BASE_OFFSET, &self.pending)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICPENDR_BASE_OFFSET, &self.pending)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ISACTIVER_BASE_OFFSET, &self.active)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICACTIVER_BASE_OFFSET, &self.active)
                {
                    return action;
                }
                if let Some(action) = Self::read_byte_indexed_register(
                    current,
                    GICD_IPRIORITYR_BASE_OFFSET,
                    &self.priority,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICFGR_BASE_OFFSET, &self.config)
                {
                    return action;
                }
                if let Some(action) = Self::read_indexed_register(
                    current,
                    GICD_IGRPMODR_BASE_OFFSET,
                    &self.group_modifier,
                ) {
                    return action;
                }
                if let Some(interrupt_id) = Self::irouter_interrupt_id(current) {
                    let base = GICD_IROUTER_BASE_OFFSET + (interrupt_id as u64 * 8);
                    if let Some(value) = Self::read_u64_register(
                        current,
                        base,
                        self.route[interrupt_id],
                        access.width,
                    ) {
                        return MmioAction::ReadValue(value);
                    }
                }
                MmioAction::Unhandled
            }
            (MmioAccessKind::Write, GICD_CTLR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width) as u32;
                self.ctlr = value;
                MmioAction::WriteAccepted {
                    value: u64::from(value),
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, GICD_STATUSR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width) as u32;
                self.statusr &= !value;
                MmioAction::WriteAccepted {
                    value: u64::from(value),
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, current, Some(value)) => {
                if let Some(action) = Self::write_indexed_register(
                    current,
                    GICD_IGROUPR_BASE_OFFSET,
                    &mut self.group,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::set_indexed_bits(
                    current,
                    GICD_ISENABLER_BASE_OFFSET,
                    &mut self.enabled,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::clear_indexed_bits(
                    current,
                    GICD_ICENABLER_BASE_OFFSET,
                    &mut self.enabled,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::set_indexed_bits(
                    current,
                    GICD_ISPENDR_BASE_OFFSET,
                    &mut self.pending,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::clear_indexed_bits(
                    current,
                    GICD_ICPENDR_BASE_OFFSET,
                    &mut self.pending,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::set_indexed_bits(
                    current,
                    GICD_ISACTIVER_BASE_OFFSET,
                    &mut self.active,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::clear_indexed_bits(
                    current,
                    GICD_ICACTIVER_BASE_OFFSET,
                    &mut self.active,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::write_byte_indexed_register(
                    current,
                    GICD_IPRIORITYR_BASE_OFFSET,
                    &mut self.priority,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::write_indexed_register(
                    current,
                    GICD_ICFGR_BASE_OFFSET,
                    &mut self.config,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::write_indexed_register(
                    current,
                    GICD_IGRPMODR_BASE_OFFSET,
                    &mut self.group_modifier,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(interrupt_id) = Self::irouter_interrupt_id(current) {
                    let base = GICD_IROUTER_BASE_OFFSET + (interrupt_id as u64 * 8);
                    if let Some(routing) = Self::write_u64_register(
                        self.route[interrupt_id],
                        current,
                        base,
                        value,
                        access.width,
                    ) {
                        self.route[interrupt_id] = routing;
                        let value = mask_mmio_value(value, access.width);
                        return MmioAction::WriteAccepted {
                            value,
                            byte: (value & 0xff) as u8,
                        };
                    }
                }
                MmioAction::Unhandled
            }
            _ => MmioAction::Unhandled,
        }
    }
}
