//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GicV3CpuInterfaceState {
    pub(crate) sre: u64,
    pub(crate) ctlr: u64,
    pub(crate) priority_mask: u8,
    pub(crate) binary_point0: u8,
    pub(crate) binary_point1: u8,
    pub(crate) group0_enabled: bool,
    pub(crate) group1_enabled: bool,
    pub(crate) active_priority0: [u32; 4],
    pub(crate) active_priority1: [u32; 4],
    pub(crate) active_group1: Vec<GicV3ActiveInterrupt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GicV3CpuInterfaceAction {
    Read(u64),
    Write { refresh_level_sources: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GicV3CpuInterfaceIrqLineSnapshot {
    pub(crate) group1_enabled: bool,
    pub(crate) priority_mask: u8,
    pub(crate) running_priority: u8,
    pub(crate) priority_threshold: u8,
    pub(crate) pending_intid: u32,
    pub(crate) irq_line_should_assert: bool,
}

impl GicV3CpuInterfaceState {
    pub(crate) fn new() -> Self {
        Self {
            // Report system-register access as enabled for the guest-visible CPU interface.
            sre: 0x7,
            ctlr: 0,
            priority_mask: 0xff,
            binary_point0: 0,
            binary_point1: 0,
            group0_enabled: false,
            group1_enabled: false,
            active_priority0: [0; 4],
            active_priority1: [0; 4],
            active_group1: Vec::new(),
        }
    }

    pub(crate) fn irq_line_snapshot(&self, bus: &mut MmioBus) -> GicV3CpuInterfaceIrqLineSnapshot {
        let running_priority = self.running_priority();
        let priority_threshold = self.priority_mask.min(running_priority);
        let pending_intid = pending_windows_arm_firmware_gic_irq(bus, priority_threshold);
        let irq_line_should_assert =
            self.group1_enabled && pending_intid != GICV3_SPURIOUS_INTERRUPT_ID;

        GicV3CpuInterfaceIrqLineSnapshot {
            group1_enabled: self.group1_enabled,
            priority_mask: self.priority_mask,
            running_priority,
            priority_threshold,
            pending_intid,
            irq_line_should_assert,
        }
    }

    #[cfg(test)]
    pub(crate) fn irq_line_should_assert(&self, bus: &mut MmioBus) -> bool {
        self.irq_line_snapshot(bus).irq_line_should_assert
    }

    pub(crate) fn mask_write(value: u64) -> u64 {
        value & 0xffff_ffff
    }

    pub(crate) fn acknowledge_group1(&mut self, bus: &mut MmioBus) -> u32 {
        if !self.group1_enabled {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        }
        let Some(interrupt) =
            acknowledge_windows_arm_firmware_gic_irq(bus, self.group1_priority_threshold())
        else {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        };
        self.active_group1.push(GicV3ActiveInterrupt {
            interrupt_id: interrupt.interrupt_id,
            priority: interrupt.priority,
            priority_dropped: false,
        });
        interrupt.interrupt_id
    }

    pub(crate) fn highest_pending_group1(&self, bus: &mut MmioBus) -> u32 {
        if self.group1_enabled {
            pending_windows_arm_firmware_gic_irq(bus, self.group1_priority_threshold())
        } else {
            GICV3_SPURIOUS_INTERRUPT_ID
        }
    }

    pub(crate) fn running_priority(&self) -> u8 {
        self.active_group1
            .iter()
            .filter(|interrupt| !interrupt.priority_dropped)
            .map(|interrupt| interrupt.priority)
            .min()
            .unwrap_or(0xff)
    }

    pub(crate) fn group1_priority_threshold(&self) -> u8 {
        self.priority_mask.min(self.running_priority())
    }

    pub(crate) fn active_priority_register_index(sys_reg: u16) -> Option<(bool, usize)> {
        match sys_reg {
            ICC_AP0R0_EL1_SYSREG => Some((false, 0)),
            ICC_AP0R1_EL1_SYSREG => Some((false, 1)),
            ICC_AP0R2_EL1_SYSREG => Some((false, 2)),
            ICC_AP0R3_EL1_SYSREG => Some((false, 3)),
            ICC_AP1R0_EL1_SYSREG => Some((true, 0)),
            ICC_AP1R1_EL1_SYSREG => Some((true, 1)),
            ICC_AP1R2_EL1_SYSREG => Some((true, 2)),
            ICC_AP1R3_EL1_SYSREG => Some((true, 3)),
            _ => None,
        }
    }

    pub(crate) fn eoi_mode(&self) -> bool {
        self.ctlr & ICC_CTLR_EL1_EOIMODE != 0
    }

    pub(crate) fn group1_interrupt_id_from_write(value: u64) -> Option<u32> {
        let interrupt_id = (value & 0x00ff_ffff) as u32;
        if interrupt_id == GICV3_SPURIOUS_INTERRUPT_ID {
            None
        } else {
            Some(interrupt_id)
        }
    }

    pub(crate) fn priority_drop_group1(&mut self, value: u64) {
        let Some(interrupt_id) = Self::group1_interrupt_id_from_write(value) else {
            return;
        };
        if let Some(active) = self
            .active_group1
            .iter_mut()
            .rfind(|active| active.interrupt_id == interrupt_id)
        {
            active.priority_dropped = true;
        }
    }

    pub(crate) fn deactivate_group1(&mut self, bus: &mut MmioBus, value: u64) -> bool {
        let Some(interrupt_id) = Self::group1_interrupt_id_from_write(value) else {
            return false;
        };
        if let Some(position) = self
            .active_group1
            .iter()
            .rposition(|active| active.interrupt_id == interrupt_id)
        {
            self.active_group1.remove(position);
        }
        end_windows_arm_firmware_gic_irq(bus, interrupt_id)
    }

    pub(crate) fn write_eoir_group1(&mut self, bus: &mut MmioBus, value: u64) -> bool {
        self.priority_drop_group1(value);
        if self.eoi_mode() {
            false
        } else {
            self.deactivate_group1(bus, value)
        }
    }

    pub(crate) fn write_dir_group1(&mut self, bus: &mut MmioBus, value: u64) -> bool {
        self.deactivate_group1(bus, value)
    }

    pub(crate) fn handle_system_register_access(
        &mut self,
        bus: &mut MmioBus,
        access: DecodedSystemRegisterAccess,
        write_value: Option<u64>,
    ) -> Option<GicV3CpuInterfaceAction> {
        match (access.is_read, access.sys_reg) {
            (true, ICC_SRE_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(self.sre)),
            (false, ICC_SRE_EL1_SYSREG) => {
                self.sre = Self::mask_write(write_value?) | 1;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_CTLR_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(self.ctlr)),
            (false, ICC_CTLR_EL1_SYSREG) => {
                self.ctlr = Self::mask_write(write_value?);
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_PMR_EL1_SYSREG) => {
                Some(GicV3CpuInterfaceAction::Read(u64::from(self.priority_mask)))
            }
            (false, ICC_PMR_EL1_SYSREG) => {
                self.priority_mask = (write_value? & 0xff) as u8;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_RPR_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.running_priority(),
            ))),
            (true, ICC_BPR0_EL1_SYSREG) => {
                Some(GicV3CpuInterfaceAction::Read(u64::from(self.binary_point0)))
            }
            (false, ICC_BPR0_EL1_SYSREG) => {
                self.binary_point0 = (write_value? & 0x7) as u8;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_BPR1_EL1_SYSREG) => {
                Some(GicV3CpuInterfaceAction::Read(u64::from(self.binary_point1)))
            }
            (false, ICC_BPR1_EL1_SYSREG) => {
                self.binary_point1 = (write_value? & 0x7) as u8;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_IGRPEN0_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.group0_enabled as u8,
            ))),
            (false, ICC_IGRPEN0_EL1_SYSREG) => {
                self.group0_enabled = (write_value? & 1) != 0;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_IGRPEN1_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.group1_enabled as u8,
            ))),
            (false, ICC_IGRPEN1_EL1_SYSREG) => {
                self.group1_enabled = (write_value? & 1) != 0;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_HPPIR1_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.highest_pending_group1(bus),
            ))),
            (true, ICC_IAR1_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.acknowledge_group1(bus),
            ))),
            (true, ICC_HPPIR0_EL1_SYSREG | ICC_IAR0_EL1_SYSREG) => Some(
                GicV3CpuInterfaceAction::Read(u64::from(GICV3_SPURIOUS_INTERRUPT_ID)),
            ),
            (false, ICC_EOIR0_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            }),
            (false, ICC_EOIR1_EL1_SYSREG) => {
                let refresh_level_sources = self.write_eoir_group1(bus, write_value?);
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources,
                })
            }
            (false, ICC_DIR_EL1_SYSREG) => {
                let refresh_level_sources = self.write_dir_group1(bus, write_value?);
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources,
                })
            }
            (false, ICC_SGI1R_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            }),
            (is_read, sys_reg) => {
                let (group1, index) = Self::active_priority_register_index(sys_reg)?;
                if is_read {
                    let value = if group1 {
                        self.active_priority1[index]
                    } else {
                        self.active_priority0[index]
                    };
                    Some(GicV3CpuInterfaceAction::Read(u64::from(value)))
                } else {
                    let value = Self::mask_write(write_value?) as u32;
                    if group1 {
                        self.active_priority1[index] = value;
                    } else {
                        self.active_priority0[index] = value;
                    }
                    Some(GicV3CpuInterfaceAction::Write {
                        refresh_level_sources: false,
                    })
                }
            }
        }
    }
}
