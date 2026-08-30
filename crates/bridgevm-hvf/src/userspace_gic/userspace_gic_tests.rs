use super::*;

fn enable_spi(gic: &mut UserspaceGic, intid: u32, cpu_route: u64) {
    let intid = intid as usize;
    // GICD_CTLR: enable group1.
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
    // Group1, enabled, priority 0xa0, routed.
    let (reg, _) = (intid / 32, intid % 32);
    gic.mmio(
        machine::GIC_DIST.base + GICD_IGROUPR + (reg as u64) * 4,
        4,
        Some(0xffff_ffff),
    );
    gic.mmio(
        machine::GIC_DIST.base + GICD_ISENABLER + (reg as u64) * 4,
        4,
        Some(1 << (intid % 32)),
    );
    gic.mmio(
        machine::GIC_DIST.base + GICD_IPRIORITYR + intid as u64,
        1,
        Some(0xa0),
    );
    gic.mmio(
        machine::GIC_DIST.base + GICD_IROUTER + (intid as u64) * 8,
        8,
        Some(cpu_route),
    );
}

fn wake_cpu(gic: &mut UserspaceGic, cpu: usize) {
    let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64;
    gic.mmio(base + GICR_WAKER, 4, Some(0));
    // Unmask at the CPU interface.
    gic.sysreg(cpu, ICC_PMR_EL1, false, 0xff);
    gic.sysreg(cpu, ICC_IGRPEN1_EL1, false, 1);
}

fn enable_vtimer_ppi(gic: &mut UserspaceGic, cpu: usize) {
    let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64;
    gic.mmio(base + GICR_IGROUPR0, 4, Some(0xffff_ffff));
    gic.mmio(base + GICR_ISENABLER0, 4, Some(1 << VTIMER_INTID));
    gic.mmio(
        base + GICR_IPRIORITYR + u64::from(VTIMER_INTID),
        1,
        Some(0x80),
    );
}

#[test]
fn spi_level_assert_ack_eoi_cycle() {
    let mut gic = UserspaceGic::new(4);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, 40, 0); // route affinity 0 -> cpu0

    assert!(!gic.line_asserted(0));
    let kicks = gic.set_spi(40, true);
    assert_eq!(kicks, 1);
    assert!(gic.line_asserted(0));
    assert!(!gic.line_asserted(1));

    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, 40);
    // Level source still high but interrupt is active: line drops.
    assert!(!gic.line_asserted(0));

    // Guest device handler lowers the level, then EOIs.
    gic.set_spi(40, false);
    let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, 40).unwrap();
    assert_eq!(eoi.kick_mask, 1);
    assert!(!gic.line_asserted(0));

    // Re-raise: full second delivery.
    gic.set_spi(40, true);
    assert!(gic.line_asserted(0));
    let iar2 = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar2.value, 40);
}

#[test]
fn spi_level_stays_pending_while_high_after_eoi() {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, 33, 0);

    gic.set_spi(33, true);
    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, 33);
    gic.sysreg(0, ICC_EOIR1_EL1, false, 33).unwrap();
    // Level input still asserted: interrupt must re-fire (this is the
    // lost-delivery shape the in-kernel GIC dies on).
    assert!(gic.line_asserted(0));
}

#[test]
fn msi_latches_edge_pending_and_targets_router() {
    let mut gic = UserspaceGic::new(4);
    wake_cpu(&mut gic, 2);
    enable_spi(&mut gic, machine::GIC_MSI_INTID_BASE + 32, 2 << 8 | 2);
    // Fix the route to cpu2's mpidr (aff1=0,aff0=2).
    let intid = u64::from(machine::GIC_MSI_INTID_BASE) + 32;
    gic.mmio(
        machine::GIC_DIST.base + GICD_IROUTER + intid * 8,
        8,
        Some(2),
    );

    let kicks = gic.send_msi(
        machine::GIC_MSI_FRAME.base + 0x40,
        machine::GIC_MSI_INTID_BASE + 32,
    );
    assert_eq!(kicks, 1 << 2);
    assert!(gic.line_asserted(2));
    let iar = gic.sysreg(2, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, intid);
    // Edge: no re-fire after EOI without a new message.
    gic.sysreg(2, ICC_EOIR1_EL1, false, iar.value).unwrap();
    assert!(!gic.line_asserted(2));
}

#[test]
fn msi_frame_mmio_write_latches_like_send_msi() {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    let intid = machine::GIC_MSI_INTID_BASE;
    enable_spi(&mut gic, intid, 0);

    let result = gic.mmio(
        machine::GIC_MSI_FRAME.base + 0x40,
        4,
        Some(u64::from(intid)),
    );
    assert_eq!(result.kick_mask, 1);
    assert!(gic.line_asserted(0));
}

#[test]
fn vtimer_ppi_delivers_and_tracks_in_service() {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, 0);
    enable_vtimer_ppi(&mut gic, 0);

    assert!(gic.vtimer_enabled(0));
    assert!(!gic.vtimer_in_service(0));
    let kicks = gic.set_vtimer_ppi(0, true);
    assert_eq!(kicks, 1);
    assert!(gic.line_asserted(0));

    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, u64::from(VTIMER_INTID));
    assert!(gic.vtimer_in_service(0));
    assert!(!gic.line_asserted(0));

    // Guest timer handler reprograms CVAL; host lowers the level; EOI.
    gic.set_vtimer_ppi(0, false);
    gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(VTIMER_INTID))
        .unwrap();
    assert!(!gic.vtimer_in_service(0));
    assert!(!gic.line_asserted(0));

    // Next fire delivers again — RPR must be back to idle.
    let rpr = gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap();
    assert_eq!(rpr.value, 0xff);
    gic.set_vtimer_ppi(0, true);
    assert!(gic.line_asserted(0));
}

#[test]
fn eoimode_one_splits_priority_drop_and_deactivate() {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    enable_vtimer_ppi(&mut gic, 0);
    gic.sysreg(0, ICC_CTLR_EL1, false, ICC_CTLR_EOIMODE)
        .unwrap();

    gic.set_vtimer_ppi(0, true);
    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, u64::from(VTIMER_INTID));

    // EOI drops priority but interrupt stays active (blocks re-delivery).
    gic.set_vtimer_ppi(0, false);
    gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(VTIMER_INTID))
        .unwrap();
    assert!(gic.vtimer_in_service(0));
    let rpr = gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap();
    assert_eq!(rpr.value, 0xff, "priority drop must reset RPR");
    gic.set_vtimer_ppi(0, true);
    assert!(!gic.line_asserted(0), "active interrupt blocks re-delivery");

    // DIR deactivates: now it can deliver again.
    gic.sysreg(0, ICC_DIR_EL1, false, u64::from(VTIMER_INTID))
        .unwrap();
    assert!(!gic.vtimer_in_service(0));
    assert!(gic.line_asserted(0));
}

#[test]
fn sgi_reaches_targeted_cpus_and_not_self_with_irm() {
    let mut gic = UserspaceGic::new(4);
    for cpu in 0..4 {
        wake_cpu(&mut gic, cpu);
        let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64;
        gic.mmio(base + GICR_IGROUPR0, 4, Some(0xffff_ffff));
        gic.mmio(base + GICR_ISENABLER0, 4, Some(0xffff));
        for sgi in 0..16u64 {
            gic.mmio(base + GICR_IPRIORITYR + sgi, 1, Some(0x90));
        }
    }

    // cpu1 sends SGI 3 to target list {cpu0, cpu2} (aff1=0).
    let sgi1r = (3u64 << 24) | 0b0101;
    let result = gic.sysreg(1, ICC_SGI1R_EL1, false, sgi1r).unwrap();
    assert_eq!(result.kick_mask, 0b0101);
    assert!(gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
    assert!(gic.line_asserted(2));
    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, 3);

    // IRM: everyone except the sender.
    let irm = (5u64 << 24) | (1 << 40);
    let result = gic.sysreg(3, ICC_SGI1R_EL1, false, irm).unwrap();
    assert_eq!(result.kick_mask & (1 << 3), 0);
    assert!(gic.line_asserted(1));
}

#[test]
fn priority_masks_gate_delivery() {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, 50, 0);

    // PMR below the SPI priority (0xa0): must not deliver.
    gic.sysreg(0, ICC_PMR_EL1, false, 0x40);
    gic.set_spi(50, true);
    assert!(!gic.line_asserted(0));
    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, u64::from(SPURIOUS_INTID));

    // Raise PMR: delivers.
    let result = gic.sysreg(0, ICC_PMR_EL1, false, 0xf0).unwrap();
    assert_eq!(result.kick_mask, 1);
    assert!(gic.line_asserted(0));
}

#[test]
fn higher_priority_preempts_lower_in_service() {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, 60, 0);
    enable_spi(&mut gic, 61, 0);
    // 61 more important.
    gic.mmio(machine::GIC_DIST.base + GICD_IPRIORITYR + 61, 1, Some(0x40));

    gic.set_spi(60, true);
    let first = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(first.value, 60);
    assert!(!gic.line_asserted(0));

    // Higher-priority 61 preempts while 60 is in service.
    gic.set_spi(61, true);
    assert!(gic.line_asserted(0));
    let second = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(second.value, 61);
    let rpr = gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap();
    assert_eq!(rpr.value, 0x40);

    // Same-priority does not preempt.
    gic.set_spi(61, false);
    gic.sysreg(0, ICC_EOIR1_EL1, false, 61).unwrap();
    gic.set_spi(60, false);
    gic.sysreg(0, ICC_EOIR1_EL1, false, 60).unwrap();
    assert_eq!(
        gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value,
        0xff,
        "all EOId -> idle RPR"
    );
}

#[test]
fn redistributor_typer_marks_last_and_affinity() {
    let mut gic = UserspaceGic::new(3);
    let stride = machine::GICV3_REDIST_STRIDE;
    let low = gic
        .mmio(machine::GIC_REDIST.base + GICR_TYPER, 4, None)
        .value;
    assert_eq!(low & (1 << 4), 0, "cpu0 is not last of 3");
    let last = gic
        .mmio(machine::GIC_REDIST.base + stride * 2 + GICR_TYPER, 8, None)
        .value;
    assert_eq!(last & (1 << 4), 1 << 4, "cpu2 is last");
    assert_eq!((last >> 32) & 0xff_ffff, 2, "affinity aff0=2");
    assert_eq!((last >> 8) & 0xffff, 2, "processor number 2");
}

#[test]
fn waker_is_storage_only_and_never_gates_delivery() {
    // The declared platform policy keeps delivery active when firmware leaves
    // WAKER untouched; writes only round-trip the sleep bit.
    let mut gic = UserspaceGic::new(1);
    gic.sysreg(0, ICC_PMR_EL1, false, 0xff);
    gic.sysreg(0, ICC_IGRPEN1_EL1, false, 1);
    enable_vtimer_ppi(&mut gic, 0);
    gic.set_vtimer_ppi(0, true);
    assert!(gic.line_asserted(0), "delivery must not wait for WAKER");

    let base = machine::GIC_REDIST.base;
    assert_eq!(gic.mmio(base + GICR_WAKER, 4, None).value, 0);
    gic.mmio(
        base + GICR_WAKER,
        4,
        Some(u64::from(GICR_WAKER_PROCESSOR_SLEEP)),
    );
    assert_eq!(
        gic.mmio(base + GICR_WAKER, 4, None).value,
        u64::from(GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP)
    );
    assert!(gic.line_asserted(0), "sleep bit is storage, not a gate");
    gic.mmio(base + GICR_WAKER, 4, Some(0));
    assert!(gic.line_asserted(0));
}

#[test]
fn dist_registers_read_back_and_pidr2_identifies_gicv3() {
    let mut gic = UserspaceGic::new(2);
    let base = machine::GIC_DIST.base;
    assert_eq!(gic.mmio(base + GICD_PIDR2, 4, None).value, 0x30);
    let typer = gic.mmio(base + GICD_TYPER, 4, None).value;
    assert_eq!(typer & 0x1f, (GIC_INTID_COUNT as u64 / 32) - 1);

    gic.mmio(base + GICD_ISENABLER + 4, 4, Some(0x8));
    assert_eq!(gic.mmio(base + GICD_ISENABLER + 4, 4, None).value, 0x8);
    assert_eq!(gic.mmio(base + GICD_ICENABLER + 4, 4, None).value, 0x8);
    gic.mmio(base + GICD_ICENABLER + 4, 4, Some(0x8));
    assert_eq!(gic.mmio(base + GICD_ISENABLER + 4, 4, None).value, 0);

    // Priority bytes: 4-wide write, byte read-back.
    gic.mmio(base + GICD_IPRIORITYR + 40, 4, Some(0xd0c0_b0a0));
    assert_eq!(gic.mmio(base + GICD_IPRIORITYR + 41, 1, None).value, 0xb0);

    // IROUTER 32-bit halves.
    gic.mmio(base + GICD_IROUTER + 45 * 8, 4, Some(0x0000_0102));
    gic.mmio(base + GICD_IROUTER + 45 * 8 + 4, 4, Some(0));
    assert_eq!(gic.mmio(base + GICD_IROUTER + 45 * 8, 8, None).value, 0x102);
}

#[test]
fn sysreg_returns_none_for_non_gic_registers() {
    let mut gic = UserspaceGic::new(1);
    assert!(gic.sysreg(0, 0xdf19, true, 0).is_none()); // CNTV_CTL_EL0
    assert!(gic.sysreg(0, 0x0000, true, 0).is_none());
}

#[test]
fn owns_covers_dist_redist_msi_frame_only() {
    assert!(UserspaceGic::owns(machine::GIC_DIST.base));
    assert!(UserspaceGic::owns(machine::GIC_DIST.base + 0xffff));
    assert!(UserspaceGic::owns(machine::GIC_REDIST.base));
    assert!(UserspaceGic::owns(machine::GIC_MSI_FRAME.base + 0x40));
    assert!(!UserspaceGic::owns(0x0900_0000)); // UART
    assert!(!UserspaceGic::owns(0));
}

#[test]
fn unrouted_spi_falls_back_nowhere_and_irm_picks_cpu0() {
    let mut gic = UserspaceGic::new(4);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, 70, 0);
    // Route to a nonexistent affinity: no kicks, no delivery.
    gic.mmio(
        machine::GIC_DIST.base + GICD_IROUTER + 70 * 8,
        8,
        Some(0xff),
    );
    assert_eq!(gic.set_spi(70, true), 0);
    assert!(!gic.line_asserted(0));

    // BridgeVM's deterministic IRM (1-of-N) selection chooses CPU0.
    gic.mmio(
        machine::GIC_DIST.base + GICD_IROUTER + 70 * 8,
        8,
        Some(IROUTER_IRM),
    );
    // Lowering under IRM kicks too: the line must be re-evaluated down.
    assert_eq!(gic.set_spi(70, false), 1);
    assert_eq!(gic.set_spi(70, true), 1);
    assert!(gic.line_asserted(0));
}
