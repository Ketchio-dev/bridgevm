//! GIC CPU-interface acknowledge/EOI/DIR and IRQ-line gating.

use super::helpers::*;
use crate::probe_mmio::*;
use crate::*;

#[test]
fn firmware_system_register_trap_decoder_handles_gic_cpu_interface_regs() {
    let iar = decode_system_register_trap(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0))
        .expect("ICC_IAR1_EL1 trap decodes");
    assert!(iar.is_read);
    assert_eq!(iar.access_name(), "read");
    assert_eq!(iar.register, 2);
    assert_eq!(iar.sys_reg, ICC_IAR1_EL1_SYSREG);

    let eoir = decode_system_register_trap(sysreg_trap_syndrome(false, 4, 3, 0, 12, 12, 1))
        .expect("ICC_EOIR1_EL1 trap decodes");
    assert!(!eoir.is_read);
    assert_eq!(eoir.access_name(), "write");
    assert_eq!(eoir.register, 4);
    assert_eq!(eoir.sys_reg, ICC_EOIR1_EL1_SYSREG);

    assert_eq!(aarch64_sys_reg_encoding(3, 0, 4, 6, 0), ICC_PMR_EL1_SYSREG);
    assert_eq!(
        aarch64_sys_reg_encoding(3, 0, 12, 8, 4),
        ICC_AP0R0_EL1_SYSREG
    );
    assert_eq!(
        aarch64_sys_reg_encoding(3, 0, 12, 9, 0),
        ICC_AP1R0_EL1_SYSREG
    );
    assert_eq!(
        aarch64_sys_reg_encoding(3, 0, 12, 11, 3),
        ICC_RPR_EL1_SYSREG
    );
    assert_eq!(
        aarch64_sys_reg_encoding(3, 0, 12, 12, 5),
        ICC_SRE_EL1_SYSREG
    );
    assert_eq!(
        aarch64_sys_reg_encoding(3, 0, 12, 12, 6),
        ICC_IGRPEN0_EL1_SYSREG
    );
    assert_eq!(decode_system_register_trap(0x93c0_8006), None);
}

#[test]
fn gicv3_cpu_interface_accepts_group0_and_active_priority_registers() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();

    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0xff))
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_BPR0_EL1_SYSREG, 0x9),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_BPR0_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(1))
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN0_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IGRPEN0_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(1))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR0_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            GICV3_SPURIOUS_INTERRUPT_ID
        )))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR0_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            GICV3_SPURIOUS_INTERRUPT_ID
        )))
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_EOIR0_EL1_SYSREG, 0),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_AP0R0_EL1_SYSREG, 0x1234),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_AP0R0_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0x1234))
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_AP1R0_EL1_SYSREG, 0x5678),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_AP1R0_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0x5678))
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_SGI1R_EL1_SYSREG, 0),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
}

#[test]
fn gicv3_cpu_interface_acknowledges_and_eois_pending_device_spis() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            base + GICD_CTLR_OFFSET,
            u64::from(GICD_CTLR_ENABLE_GRP1NS),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
            byte: GICD_CTLR_ENABLE_GRP1NS as u8,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            base + GICD_ISENABLER_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            base + GICD_ISPENDR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        cpu.handle_system_register_access(
            &mut bus,
            DecodedSystemRegisterAccess {
                is_read: false,
                register: 0,
                sys_reg: ICC_IGRPEN1_EL1_SYSREG,
                op0: 3,
                op1: 0,
                crn: 12,
                crm: 12,
                op2: 7,
            },
            Some(1),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            base + GICD_IGROUPR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert!(cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        cpu.handle_system_register_access(
            &mut bus,
            DecodedSystemRegisterAccess {
                is_read: false,
                register: 0,
                sys_reg: ICC_PMR_EL1_SYSREG,
                op0: 3,
                op1: 0,
                crn: 4,
                crm: 6,
                op2: 0,
            },
            Some(0xa0),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        cpu.handle_system_register_access(
            &mut bus,
            DecodedSystemRegisterAccess {
                is_read: false,
                register: 0,
                sys_reg: ICC_PMR_EL1_SYSREG,
                op0: 3,
                op1: 0,
                crn: 4,
                crm: 6,
                op2: 0,
            },
            Some(0xff),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert!(cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        cpu.handle_system_register_access(
            &mut bus,
            DecodedSystemRegisterAccess {
                is_read: true,
                register: 1,
                sys_reg: ICC_HPPIR1_EL1_SYSREG,
                op0: 3,
                op1: 0,
                crn: 12,
                crm: 12,
                op2: 2,
            },
            None,
        ),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0xff))
    );
    assert_eq!(
        cpu.handle_system_register_access(
            &mut bus,
            DecodedSystemRegisterAccess {
                is_read: true,
                register: 1,
                sys_reg: ICC_IAR1_EL1_SYSREG,
                op0: 3,
                op1: 0,
                crn: 12,
                crm: 12,
                op2: 0,
            },
            None,
        ),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0xa0))
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        bus.dispatch(MmioAccess::read(base + GICD_ISPENDR_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0x4)
    );
    assert_eq!(
        cpu.handle_system_register_access(
            &mut bus,
            DecodedSystemRegisterAccess {
                is_read: false,
                register: 1,
                sys_reg: ICC_EOIR1_EL1_SYSREG,
                op0: 3,
                op1: 0,
                crn: 12,
                crm: 12,
                op2: 1,
            },
            Some(34),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: true,
        })
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0xff))
    );
}

#[test]
fn gicv3_cpu_interface_acknowledges_and_eois_timer_ppi() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert!(cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xa0),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xff),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
        )))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
        )))
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISPENDR0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(timer_bit))
    );
    assert_eq!(
        gic_cpu_write(
            &mut cpu,
            &mut bus,
            ICC_EOIR1_EL1_SYSREG,
            u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: true,
        })
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
}

#[test]
fn gicv3_cpu_interface_irq_line_snapshot_reports_timer_ppi_gates() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));

    let snapshot = cpu.irq_line_snapshot(&mut bus);
    assert!(!snapshot.group1_enabled);
    assert_eq!(snapshot.priority_mask, 0xff);
    assert_eq!(snapshot.running_priority, 0xff);
    assert_eq!(snapshot.priority_threshold, 0xff);
    assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
    assert!(!snapshot.irq_line_should_assert);

    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    let snapshot = cpu.irq_line_snapshot(&mut bus);
    assert!(snapshot.group1_enabled);
    assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
    assert!(!snapshot.irq_line_should_assert);

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    let snapshot = cpu.irq_line_snapshot(&mut bus);
    assert_eq!(
        snapshot.pending_intid,
        WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
    );
    assert!(snapshot.irq_line_should_assert);

    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xa0),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    let snapshot = cpu.irq_line_snapshot(&mut bus);
    assert_eq!(snapshot.priority_mask, 0xa0);
    assert_eq!(snapshot.priority_threshold, 0xa0);
    assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
    assert!(!snapshot.irq_line_should_assert);
}

#[test]
fn gicv3_cpu_interface_timer_ppi_does_not_clear_pending_spi_line() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let gicd_base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;
    let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicd_base + GICD_CTLR_OFFSET,
            u64::from(GICD_CTLR_ENABLE_GRP1NS),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
            byte: GICD_CTLR_ENABLE_GRP1NS as u8,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicd_base + GICD_IGROUPR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicd_base + GICD_ISENABLER_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicd_base + GICD_ISPENDR_BASE_OFFSET + 4,
            0x4,
            4,
        )),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );

    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
        )))
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            GICV3_SPURIOUS_INTERRUPT_ID
        )))
    );
    assert_eq!(
        gic_cpu_write(
            &mut cpu,
            &mut bus,
            ICC_EOIR1_EL1_SYSREG,
            u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: true,
        })
    );
    assert!(cpu.irq_line_should_assert(&mut bus));
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert!(!cpu.irq_line_should_assert(&mut bus));
}

#[test]
fn gicv3_cpu_interface_selects_highest_priority_pending_across_ppi_and_spi() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let gicd_base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;
    let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicd_base + GICD_CTLR_OFFSET,
            u64::from(GICD_CTLR_ENABLE_GRP1NS),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
            byte: GICD_CTLR_ENABLE_GRP1NS as u8,
        }
    );
    for register_base in [
        GICD_IGROUPR_BASE_OFFSET,
        GICD_ISENABLER_BASE_OFFSET,
        GICD_ISPENDR_BASE_OFFSET,
    ] {
        assert_eq!(
            bus.dispatch(MmioAccess::write(gicd_base + register_base + 4, 0x4, 4,)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
    }
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicd_base + GICD_IPRIORITYR_BASE_OFFSET + 34,
            0x20,
            1,
        )),
        MmioAction::WriteAccepted {
            value: 0x20,
            byte: 0x20,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );

    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0x20))
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISPENDR0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(timer_bit))
    );
}

#[test]
fn gicv3_cpu_interface_eoi_mode_requires_dir_to_deactivate_spi() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            base + GICD_CTLR_OFFSET,
            u64::from(GICD_CTLR_ENABLE_GRP1NS),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
            byte: GICD_CTLR_ENABLE_GRP1NS as u8,
        }
    );
    for register_base in [
        GICD_IGROUPR_BASE_OFFSET,
        GICD_ISENABLER_BASE_OFFSET,
        GICD_ISPENDR_BASE_OFFSET,
    ] {
        assert_eq!(
            bus.dispatch(MmioAccess::write(base + register_base + 4, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
    }
    assert_eq!(
        gic_cpu_write(
            &mut cpu,
            &mut bus,
            ICC_CTLR_EL1_SYSREG,
            ICC_CTLR_EL1_EOIMODE
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(34))
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0x4)
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_EOIR1_EL1_SYSREG, 34),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0xff))
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0x4)
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_DIR_EL1_SYSREG, 34),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: true,
        })
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0)
    );
}

#[test]
fn gicv3_cpu_interface_eoi_mode_requires_dir_to_deactivate_timer_ppi() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut cpu = GicV3CpuInterfaceState::new();
    let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(
            gicr_base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
    assert_eq!(
        gic_cpu_write(
            &mut cpu,
            &mut bus,
            ICC_CTLR_EL1_SYSREG,
            ICC_CTLR_EL1_EOIMODE
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(u64::from(
            WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
        )))
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(timer_bit))
    );
    assert_eq!(
        gic_cpu_write(
            &mut cpu,
            &mut bus,
            ICC_EOIR1_EL1_SYSREG,
            u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: false,
        })
    );
    assert_eq!(
        gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
        Some(GicV3CpuInterfaceAction::Read(0xff))
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(timer_bit))
    );
    assert_eq!(
        gic_cpu_write(
            &mut cpu,
            &mut bus,
            ICC_DIR_EL1_SYSREG,
            u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
        ),
        Some(GicV3CpuInterfaceAction::Write {
            refresh_level_sources: true,
        })
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
}
