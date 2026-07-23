//! GIC distributor and redistributor register/priority behaviour.

use crate::probe_mmio::*;
use crate::*;

#[test]
fn firmware_mmio_bus_uses_windows_device_window_layout() {
    let mut bus = windows_arm_firmware_mmio_bus();

    assert_eq!(bus.device_count(), 6);
    assert!(windows_arm_device_mmio_contains(WINDOWS_ARM_PL011_MMIO_IPA));
    assert!(windows_arm_device_mmio_contains(WINDOWS_ARM_PL031_MMIO_IPA));
    assert!(windows_arm_device_mmio_contains(
        WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA
    ));
    assert!(windows_arm_device_mmio_contains(
        WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA
    ));
    assert!(windows_arm_device_mmio_contains(
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
    ));
    assert!(windows_arm_device_mmio_contains(
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
    ));
    assert!(!windows_arm_device_mmio_contains(WINDOWS_ARM_GUEST_RAM_IPA));
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_PL011_MMIO_IPA + PL011_FR_OFFSET,
            4
        )),
        MmioAction::ReadValue(WINDOWS_ARM_PL011_FLAG_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::write(WINDOWS_ARM_PL011_MMIO_IPA, 0x141, 4)),
        MmioAction::WriteAccepted {
            value: 0x141,
            byte: 0x41
        }
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(WINDOWS_ARM_PL031_MMIO_IPA, 4)),
        MmioAction::ReadValue(WINDOWS_ARM_PL031_READ_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_TYPER_OFFSET,
            4
        )),
        MmioAction::ReadValue(GICD_TYPER_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IIDR_OFFSET,
            4
        )),
        MmioAction::ReadValue(GICV3_IIDR_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA + GICR_TYPER_OFFSET,
            4
        )),
        MmioAction::ReadValue(GICR_TYPER_VALUE & 0xffff_ffff)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA + GICR_IIDR_OFFSET,
            4
        )),
        MmioAction::ReadValue(GICV3_IIDR_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
            4
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
            4
        )),
        MmioAction::ReadValue(VIRTIO_BLK_F_RO)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
            4
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
    );
    assert_eq!(
        bus.dispatch(MmioAccess::read(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
            4
        )),
        MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE)
    );
}

#[test]
fn gicv3_distributor_mmio_skeleton_tracks_common_firmware_registers() {
    let mut gic = GicV3DistributorDevice::new(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA);
    let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_TYPER_OFFSET, 4)),
        MmioAction::ReadValue(GICD_TYPER_VALUE)
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_IIDR_OFFSET, 4)),
        MmioAction::ReadValue(GICV3_IIDR_VALUE)
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_STATUSR_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gic.handle(MmioAccess::write(base + GICD_STATUSR_OFFSET, 0xff, 4)),
        MmioAction::WriteAccepted {
            value: 0xff,
            byte: 0xff
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_STATUSR_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gic.handle(MmioAccess::write(base + GICD_CTLR_OFFSET, 0x13, 4)),
        MmioAction::WriteAccepted {
            value: 0x13,
            byte: 0x13
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_CTLR_OFFSET, 4)),
        MmioAction::ReadValue(0x13)
    );

    let spi_enable_offset = GICD_ISENABLER_BASE_OFFSET + 4;
    let spi_clear_offset = GICD_ICENABLER_BASE_OFFSET + 4;
    let spi_group_offset = GICD_IGROUPR_BASE_OFFSET + 4;
    let spi_group_modifier_offset = GICD_IGRPMODR_BASE_OFFSET + 4;
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_group_modifier_offset, 0x4, 4)),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + spi_group_modifier_offset, 4)),
        MmioAction::ReadValue(0x4)
    );
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_enable_offset, 0x9, 4)),
        MmioAction::WriteAccepted {
            value: 0x9,
            byte: 0x9
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
        MmioAction::ReadValue(0x9)
    );
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_clear_offset, 0x1, 4)),
        MmioAction::WriteAccepted {
            value: 0x1,
            byte: 0x1
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
        MmioAction::ReadValue(0x8)
    );
    assert_eq!(
        GicV3DistributorDevice::spi_interrupt_id(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI),
        Some(34)
    );
    assert_eq!(GicV3DistributorDevice::interrupt_bit(34), Some((1, 0x4)));
    assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_enable_offset, 0x4, 4)),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
        MmioAction::ReadValue(0xc)
    );
    assert_eq!(
        gic.set_spi_pending(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI, true),
        Some(())
    );
    assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_group_offset, 0x4, 4)),
        MmioAction::WriteAccepted {
            value: 0x4,
            byte: 0x4
        }
    );
    assert!(gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
    assert_eq!(
        gic.set_spi_pending(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI, false),
        Some(())
    );
    assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));

    let spi_pending_set_offset = GICD_ISPENDR_BASE_OFFSET + 4;
    let spi_pending_clear_offset = GICD_ICPENDR_BASE_OFFSET + 4;
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_pending_set_offset, 0x2, 4)),
        MmioAction::WriteAccepted {
            value: 0x2,
            byte: 0x2
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + spi_pending_set_offset, 4)),
        MmioAction::ReadValue(0x2)
    );
    assert_eq!(
        gic.handle(MmioAccess::write(base + spi_pending_clear_offset, 0x2, 4)),
        MmioAction::WriteAccepted {
            value: 0x2,
            byte: 0x2
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + spi_pending_set_offset, 4)),
        MmioAction::ReadValue(0)
    );

    let priority_byte_offset = GICD_IPRIORITYR_BASE_OFFSET + 35;
    assert_eq!(
        gic.handle(MmioAccess::write(base + priority_byte_offset, 0x44, 1)),
        MmioAction::WriteAccepted {
            value: 0x44,
            byte: 0x44
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + priority_byte_offset, 1)),
        MmioAction::ReadValue(0x44)
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_IPRIORITYR_BASE_OFFSET + 32, 4)),
        MmioAction::ReadValue(0x44a0_a0a0)
    );

    let router32 = GICD_IROUTER_BASE_OFFSET + (32 * 8);
    assert_eq!(
        gic.handle(MmioAccess::write(base + router32, 0x1122_3344, 4)),
        MmioAction::WriteAccepted {
            value: 0x1122_3344,
            byte: 0x44
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::write(base + router32 + 4, 0x5566_7788, 4)),
        MmioAction::WriteAccepted {
            value: 0x5566_7788,
            byte: 0x88
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + router32, 4)),
        MmioAction::ReadValue(0x1122_3344)
    );
    assert_eq!(
        gic.handle(MmioAccess::read(base + router32 + 4, 4)),
        MmioAction::ReadValue(0x5566_7788)
    );
}

#[test]
fn gicv3_distributor_selects_pending_spi_by_priority_not_lowest_intid() {
    let mut gic = GicV3DistributorDevice::new(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA);
    let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

    assert_eq!(
        gic.handle(MmioAccess::write(
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
            gic.handle(MmioAccess::write(base + register_base + 4, 0xc, 4)),
            MmioAction::WriteAccepted {
                value: 0xc,
                byte: 0xc,
            }
        );
    }
    assert_eq!(
        gic.handle(MmioAccess::write(
            base + GICD_IPRIORITYR_BASE_OFFSET + 34,
            0xa0,
            1,
        )),
        MmioAction::WriteAccepted {
            value: 0xa0,
            byte: 0xa0,
        }
    );
    assert_eq!(
        gic.handle(MmioAccess::write(
            base + GICD_IPRIORITYR_BASE_OFFSET + 35,
            0x20,
            1,
        )),
        MmioAction::WriteAccepted {
            value: 0x20,
            byte: 0x20,
        }
    );

    assert_eq!(gic.pending_interrupt_id_for_cpu(0xff), Some(35));
    assert_eq!(gic.acknowledge_pending_interrupt(0xff), 35);
    assert_eq!(
        gic.handle(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
        MmioAction::ReadValue(0x8)
    );
    assert_eq!(gic.pending_interrupt_id_for_cpu(0xff), Some(34));
}

#[test]
fn gicv3_redistributor_mmio_skeleton_tracks_waker_and_ppi_state() {
    let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
    let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;

    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_TYPER_OFFSET, 4)),
        MmioAction::ReadValue(GICR_TYPER_VALUE & 0xffff_ffff)
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_IIDR_OFFSET, 4)),
        MmioAction::ReadValue(GICV3_IIDR_VALUE)
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_STATUSR_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(base + GICR_STATUSR_OFFSET, 0x80, 4)),
        MmioAction::WriteAccepted {
            value: 0x80,
            byte: 0x80
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_WAKER_OFFSET,
            GICR_WAKER_PROCESSOR_SLEEP,
            4
        )),
        MmioAction::WriteAccepted {
            value: GICR_WAKER_PROCESSOR_SLEEP,
            byte: GICR_WAKER_PROCESSOR_SLEEP as u8
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
        MmioAction::ReadValue(GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP)
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(base + GICR_WAKER_OFFSET, 0, 4)),
        MmioAction::WriteAccepted { value: 0, byte: 0 }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );

    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ISENABLER0_OFFSET,
            1 << 13,
            4
        )),
        MmioAction::WriteAccepted {
            value: 1 << 13,
            byte: 0
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISENABLER0_OFFSET, 4)),
        MmioAction::ReadValue(1 << 13)
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ICENABLER0_OFFSET,
            1 << 13,
            4
        )),
        MmioAction::WriteAccepted {
            value: 1 << 13,
            byte: 0
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISENABLER0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_IGRPMODR0_OFFSET,
            1 << 13,
            4
        )),
        MmioAction::WriteAccepted {
            value: 1 << 13,
            byte: 0
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_IGRPMODR0_OFFSET, 4)),
        MmioAction::ReadValue(1 << 13)
    );

    let priority_byte_offset = GICR_SGI_IPRIORITYR_BASE_OFFSET + 13;
    assert_eq!(
        gicr.handle(MmioAccess::write(base + priority_byte_offset, 0x55, 1)),
        MmioAction::WriteAccepted {
            value: 0x55,
            byte: 0x55
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + priority_byte_offset, 1)),
        MmioAction::ReadValue(0x55)
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(
            base + GICR_SGI_IPRIORITYR_BASE_OFFSET + 12,
            4
        )),
        MmioAction::ReadValue(0xa0a0_55a0)
    );

    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_PROPBASER_OFFSET,
            0x4000_0000,
            4
        )),
        MmioAction::WriteAccepted {
            value: 0x4000_0000,
            byte: 0
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_PROPBASER_OFFSET, 4)),
        MmioAction::ReadValue(0x4000_0000)
    );
}

#[test]
fn gicv3_redistributor_tracks_virtual_timer_ppi_delivery_state() {
    let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
    let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;
    let priority_byte_offset =
        GICR_SGI_IPRIORITYR_BASE_OFFSET + u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID);

    assert_eq!(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID, 27);
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ISPENDR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(timer_bit))
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(base + priority_byte_offset, 0x40, 1)),
        MmioAction::WriteAccepted {
            value: 0x40,
            byte: 0x40,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + priority_byte_offset, 1)),
        MmioAction::ReadValue(0x40)
    );
    assert_eq!(
        gicr.acknowledge_pending_interrupt(0xff),
        GICV3_SPURIOUS_INTERRUPT_ID
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(gicr.acknowledge_pending_interrupt(0xff), 27);
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(timer_bit))
    );
    assert!(gicr.end_interrupt(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID));
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
    assert!(gicr.set_fdt_ppi_pending(WINDOWS_ARM_VIRTUAL_TIMER_PPI, true));
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ICPENDR0_OFFSET,
            u64::from(timer_bit),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(timer_bit),
            byte: 0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
        MmioAction::ReadValue(0)
    );
}

#[test]
fn gicv3_redistributor_selects_pending_ppi_by_priority_not_lowest_intid() {
    let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
    let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
    let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;
    let other_ppi_interrupt_id = WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID + 1;
    let other_ppi_bit = 1_u32 << other_ppi_interrupt_id;
    let both_bits = timer_bit | other_ppi_bit;

    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_IGROUPR0_OFFSET,
            u64::from(both_bits),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(both_bits),
            byte: 0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ISENABLER0_OFFSET,
            u64::from(both_bits),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(both_bits),
            byte: 0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_ISPENDR0_OFFSET,
            u64::from(both_bits),
            4,
        )),
        MmioAction::WriteAccepted {
            value: u64::from(both_bits),
            byte: 0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_IPRIORITYR_BASE_OFFSET
                + u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            0xa0,
            1,
        )),
        MmioAction::WriteAccepted {
            value: 0xa0,
            byte: 0xa0,
        }
    );
    assert_eq!(
        gicr.handle(MmioAccess::write(
            base + GICR_SGI_IPRIORITYR_BASE_OFFSET + u64::from(other_ppi_interrupt_id),
            0x20,
            1,
        )),
        MmioAction::WriteAccepted {
            value: 0x20,
            byte: 0x20,
        }
    );

    assert_eq!(
        gicr.pending_interrupt_id_for_cpu(0xff),
        Some(other_ppi_interrupt_id)
    );
    assert_eq!(
        gicr.acknowledge_pending_interrupt(0xff),
        other_ppi_interrupt_id
    );
    assert_eq!(
        gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
        MmioAction::ReadValue(u64::from(other_ppi_bit))
    );
    assert_eq!(
        gicr.pending_interrupt_id_for_cpu(0xff),
        Some(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID)
    );
}
