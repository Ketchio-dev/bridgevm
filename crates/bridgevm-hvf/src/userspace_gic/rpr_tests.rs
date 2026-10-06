//! Arm IHI0069G 12.2.19: eight-bit RPR excludes bit0, but idle remains0xff.
//! Fixtures keep BPR1 at its minimum before acknowledgement. They do not
//! prescribe coarse-BPR activation capture or claim APR save/restore coherence.

use super::*;

fn read(gic: &mut UserspaceGic, cpu: usize, reg: u16) -> u64 {
    gic.sysreg(cpu, reg, true, 0).unwrap().value
}

fn configured() -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    for cpu in 0..2 {
        wake_cpu(&mut gic, cpu);
        gic.sysreg(cpu, ICC_BPR1_EL1, false, 1);
        assert_eq!(read(&mut gic, cpu, ICC_BPR1_EL1), 1);
        assert_eq!((read(&mut gic, cpu, ICC_CTLR_EL1) >> 8) & 7, 7);
        assert_eq!(read(&mut gic, cpu, ICC_RPR_EL1), 0xff);
    }
    gic
}

fn spi(gic: &mut UserspaceGic, cpu: usize, intid: u32, priority: u64) {
    enable_spi(gic, intid, cpu as u64);
    gic.mmio(
        machine::GIC_DIST.base + GICD_IPRIORITYR + u64::from(intid),
        1,
        Some(priority),
    );
}

fn activate(gic: &mut UserspaceGic, cpu: usize, intid: u32) {
    gic.set_spi(intid, true);
    assert_eq!(read(gic, cpu, ICC_IAR1_EL1), u64::from(intid));
    gic.set_spi(intid, false);
}

fn active_spis(gic: &mut UserspaceGic) -> u64 {
    gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None)
        .value
}

#[test]
fn odd_running_priorities_exclude_subpriority_bit_zero() {
    for priority in (1..0xffu64).step_by(2) {
        let mut gic = configured();
        spi(&mut gic, 1, 60, priority);
        activate(&mut gic, 1, 60);
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), priority - 1);
        gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
    }
}

#[test]
fn even_running_priorities_and_idle_keep_their_values() {
    for priority in (0..0xffu64).step_by(2) {
        let mut gic = configured();
        spi(&mut gic, 1, 60, priority);
        activate(&mut gic, 1, 60);
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), priority);
        gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
        assert_eq!(active_spis(&mut gic), 0);
    }
}

#[test]
fn nested_preemption_restores_the_outer_running_priority() {
    let mut gic = configured();
    spi(&mut gic, 1, 60, 0x4f);
    spi(&mut gic, 1, 61, 0x21);
    activate(&mut gic, 1, 60);
    activate(&mut gic, 1, 61);
    assert_eq!(active_spis(&mut gic), (1 << 28) | (1 << 29));
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x20);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 61);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x4e);
    assert_eq!(active_spis(&mut gic), 1 << 28);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
}

#[test]
fn split_eoi_drops_running_priority_before_deactivation() {
    let mut gic = configured();
    gic.sysreg(1, ICC_CTLR_EL1, false, ICC_CTLR_EOIMODE);
    spi(&mut gic, 1, 60, 0x81);
    activate(&mut gic, 1, 60);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x80);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
    assert_eq!(active_spis(&mut gic), 1 << 28);
    gic.sysreg(1, ICC_DIR_EL1, false, 60);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
    assert_eq!(active_spis(&mut gic), 0);
}

#[test]
fn sgi_and_ppi_running_priority_use_the_same_readback_granularity() {
    for intid in [5, VTIMER_INTID] {
        let mut gic = configured();
        let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE;
        gic.mmio(base + GICR_ISENABLER0, 4, Some(1 << intid));
        gic.mmio(base + GICR_IPRIORITYR + u64::from(intid), 1, Some(0x61));
        if intid == VTIMER_INTID {
            gic.set_vtimer_ppi(1, true);
        } else {
            gic.sysreg(0, ICC_SGI1R_EL1, false, (u64::from(intid) << 24) | 2);
        }
        assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), u64::from(intid));
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x60);
        gic.sysreg(1, ICC_EOIR1_EL1, false, u64::from(intid));
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
    }
}

#[test]
fn running_priority_is_per_cpu() {
    let mut gic = configured();
    spi(&mut gic, 0, 60, 0x91);
    spi(&mut gic, 1, 61, 0x51);
    activate(&mut gic, 0, 60);
    activate(&mut gic, 1, 61);
    assert_eq!(read(&mut gic, 0, ICC_RPR_EL1), 0x90);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x50);
    gic.sysreg(0, ICC_EOIR1_EL1, false, 60);
    assert_eq!(read(&mut gic, 0, ICC_RPR_EL1), 0xff);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x50);
}

#[test]
fn full_pending_priority_still_controls_pmr_and_same_group_blocking() {
    let mut gic = configured();
    spi(&mut gic, 1, 60, 0x41);
    spi(&mut gic, 1, 61, 0x40);
    gic.sysreg(1, ICC_PMR_EL1, false, 0x41);
    gic.set_spi(60, true);
    assert!(!gic.line_asserted(1));
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 1023);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0xff);
    gic.sysreg(1, ICC_PMR_EL1, false, 0x42);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 60);
    gic.set_spi(60, false);
    gic.set_spi(61, true);
    assert!(!gic.line_asserted(1));
    assert_eq!(read(&mut gic, 1, ICC_HPPIR1_EL1), 61);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 1023);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 61);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x40);
}

#[test]
fn repeated_running_priority_reads_preserve_pending_and_active_state() {
    let mut gic = configured();
    spi(&mut gic, 1, 60, 0x4f);
    spi(&mut gic, 1, 61, 0x60);
    activate(&mut gic, 1, 60);
    gic.set_spi(61, true);
    for _ in 0..3 {
        assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), 0x4e);
        assert_eq!(active_spis(&mut gic), 1 << 28);
        assert_eq!(read(&mut gic, 1, ICC_HPPIR1_EL1), 61);
        assert!(!gic.line_asserted(1));
    }
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 61);
}
