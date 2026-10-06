//! Targeted SGIs must not alias a different Aff2 cluster (Arm IHI0069G 12.2.21).

use super::*;

const SGI: u32 = 3;

fn redist(cpu: usize) -> u64 {
    machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64
}

fn configured(cpus: usize, enabled: bool) -> UserspaceGic {
    let mut gic = UserspaceGic::new(cpus);
    for cpu in 0..cpus {
        wake_cpu(&mut gic, cpu);
        gic.mmio(redist(cpu) + GICR_IGROUPR0, 4, Some(u32::MAX.into()));
        gic.mmio(
            redist(cpu) + GICR_IPRIORITYR + u64::from(SGI),
            1,
            Some(0x80),
        );
        if enabled {
            gic.mmio(redist(cpu) + GICR_ISENABLER0, 4, Some(1 << SGI));
        }
    }
    gic
}

fn send(gic: &mut UserspaceGic, sender: usize, affinity: u64, targets: u16) -> u64 {
    gic.sysreg(
        sender,
        ICC_SGI1R_EL1,
        false,
        affinity | (u64::from(SGI) << 24) | u64::from(targets),
    )
    .unwrap()
    .kick_mask
}

fn assert_quiet(gic: &mut UserspaceGic) {
    for cpu in 0..gic.num_cpus() {
        assert_eq!(gic.mmio(redist(cpu) + GICR_ISPENDR0, 4, None).value, 0);
        assert_eq!(gic.mmio(redist(cpu) + GICR_ISACTIVER0, 4, None).value, 0);
        assert!(!gic.line_asserted(cpu));
        assert_eq!(
            gic.sysreg(cpu, ICC_HPPIR1_EL1, true, 0).unwrap().value,
            1023
        );
        assert_eq!(gic.sysreg(cpu, ICC_IAR1_EL1, true, 0).unwrap().value, 1023);
    }
}

#[test]
fn absent_aff2_target_does_not_alias_a_real_cpu() {
    for aff2 in 1..=255u64 {
        let mut gic = configured(4, true);
        assert_eq!(send(&mut gic, 0, aff2 << 32, 2), 0, "Aff2={aff2}");
        assert_quiet(&mut gic);
    }
}

#[test]
fn absent_aff2_cluster_does_not_fan_out_to_all_sixteen_cpus() {
    let mut gic = configured(16, true);
    assert_eq!(send(&mut gic, 7, 0xff << 32, u16::MAX), 0);
    assert_quiet(&mut gic);
}

#[test]
fn absent_aff2_does_not_latch_pending_behind_active_sgi() {
    let mut gic = configured(2, true);
    assert_eq!(send(&mut gic, 0, 0, 2), 2);
    assert_eq!(
        gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value,
        u64::from(SGI)
    );
    assert_eq!(send(&mut gic, 0, 1 << 32, 2), 0);
    assert_eq!(gic.mmio(redist(1) + GICR_ISPENDR0, 4, None).value, 0);
    assert_eq!(
        gic.mmio(redist(1) + GICR_ISACTIVER0, 4, None).value,
        1 << SGI
    );
    gic.sysreg(1, ICC_EOIR1_EL1, false, u64::from(SGI));
    assert_quiet(&mut gic);
}

#[test]
fn absent_aff2_does_not_leave_a_latent_disabled_sgi() {
    let mut gic = configured(2, false);
    assert_eq!(send(&mut gic, 0, 1 << 32, 2), 0);
    assert_quiet(&mut gic);
    gic.mmio(redist(1) + GICR_ISENABLER0, 4, Some(1 << SGI));
    assert_quiet(&mut gic);
}

#[test]
fn valid_aff2_zero_target_list_preserves_exact_delivery_and_sender_selection() {
    let mut gic = configured(16, true);
    let targets = 0xa5a5u16;
    assert_eq!(send(&mut gic, 0, 0, targets), u64::from(targets));
    for cpu in 0..16 {
        let selected = targets & (1 << cpu) != 0;
        assert_eq!(gic.line_asserted(cpu), selected);
        assert_eq!(
            gic.mmio(redist(cpu) + GICR_ISPENDR0, 4, None).value,
            if selected { 1 << SGI } else { 0 }
        );
        assert_eq!(
            gic.sysreg(cpu, ICC_IAR1_EL1, true, 0).unwrap().value,
            if selected { u64::from(SGI) } else { 1023 }
        );
        if selected {
            gic.sysreg(cpu, ICC_EOIR1_EL1, false, u64::from(SGI));
        }
    }
    assert_quiet(&mut gic);
}

#[test]
fn absent_aff1_remains_ignored() {
    let mut gic = configured(4, true);
    assert_eq!(send(&mut gic, 0, 1 << 16, u16::MAX), 0);
    assert_quiet(&mut gic);
}

#[test]
fn empty_target_list_remains_ignored() {
    let mut gic = configured(4, true);
    assert_eq!(send(&mut gic, 0, 0, 0), 0);
    assert_quiet(&mut gic);
}

#[test]
fn irm_broadcast_preserves_all_but_self_delivery() {
    let mut gic = configured(4, true);
    // IRM requires the affinity and target-list RES0 fields to be zero.
    assert_eq!(send(&mut gic, 2, 1 << 40, 0), 0b1011);
    for cpu in 0..4 {
        assert_eq!(
            gic.sysreg(cpu, ICC_IAR1_EL1, true, 0).unwrap().value,
            if cpu == 2 { 1023 } else { u64::from(SGI) }
        );
    }
}
