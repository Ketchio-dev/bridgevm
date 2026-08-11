use super::*;

fn all_open() -> GicIrqState {
    GicIrqState {
        igroupr0: Some(u64::from(VTIMER_PPI_BIT)),
        isenabler0: Some(u64::from(VTIMER_PPI_BIT)),
        ispendr0: Some(u64::from(VTIMER_PPI_BIT)),
        isactiver0: Some(0),
        pmr: Some(0xf0),
        igrpen0: Some(0),
        igrpen1: Some(1),
        ctlr: Some(0),
        rpr: Some(0xff),
        ap0r0: Some(0),
        ap1r0: Some(0),
        ich_hcr: Some(1),
        ich_misr: Some(0),
        ich_elrsr: Some(0xffff),
        ich_vmcr: Some(0),
        ich_lrs: Vec::new(),
    }
}

#[test]
fn a_deliverable_pending_vtimer_points_away_from_the_gic() {
    assert_eq!(
        all_open().vtimer_verdict(),
        Some("vtimer PPI pending and deliverable; the block is not the GIC")
    );
}

#[test]
fn a_disabled_ppi_is_named_before_anything_else() {
    let state = GicIrqState {
        isenabler0: Some(0),
        // Even pending + group-on + open PMR: disabled wins.
        ..all_open()
    };
    assert_eq!(
        state.vtimer_verdict(),
        Some("vtimer PPI disabled at GICR_ISENABLER0")
    );
}

#[test]
fn enabled_but_not_pending_is_the_no_interrupt_generated_shape() {
    // This is what t4-soak's failing boots should show if the timer never
    // fires at all -- the question the host counters could not answer.
    let state = GicIrqState {
        ispendr0: Some(0),
        ..all_open()
    };
    assert_eq!(
        state.vtimer_verdict(),
        Some("vtimer PPI enabled but not pending at the GICR")
    );
}

#[test]
fn group_off_and_masked_pmr_each_get_their_own_verdict() {
    let group_off = GicIrqState {
        igrpen1: Some(0),
        ..all_open()
    };
    assert_eq!(
        group_off.vtimer_verdict(),
        Some("vtimer PPI pending but ICC group 1 is off")
    );
    let pmr_closed = GicIrqState {
        pmr: Some(0),
        ..all_open()
    };
    assert_eq!(
        pmr_closed.vtimer_verdict(),
        Some("vtimer PPI pending but ICC_PMR masks all priorities")
    );
}

#[test]
fn a_group0_ppi_is_gated_by_igrpen0_not_igrpen1() {
    // The first live capture showed the PPI configured in group 1 with
    // IGRPEN1 on; had it been in group 0, IGRPEN1's value would have been
    // irrelevant. The verdict must read the group bit, not assume group 1.
    let group0 = GicIrqState {
        igroupr0: Some(0),
        igrpen0: Some(0),
        igrpen1: Some(1),
        ..all_open()
    };
    assert_eq!(
        group0.vtimer_verdict(),
        Some("vtimer PPI pending in group 0 but ICC group 0 is off")
    );
    let group0_on = GicIrqState {
        igroupr0: Some(0),
        igrpen0: Some(1),
        igrpen1: Some(0),
        ..all_open()
    };
    assert_eq!(
        group0_on.vtimer_verdict(),
        Some("vtimer PPI pending and deliverable; the block is not the GIC")
    );
}

#[test]
fn a_failed_read_yields_no_verdict_rather_than_a_fabricated_one() {
    // The zeros-that-look-measured lesson: a refused read must not be
    // interpreted as "disabled".
    let state = GicIrqState {
        isenabler0: None,
        ..all_open()
    };
    assert_eq!(state.vtimer_verdict(), None);
    assert!(render(&state)[0].contains("ISENABLER0=?"));
    assert!(render(&state)[1].contains("unavailable"));
}

#[test]
fn render_is_bounded_and_prints_hex_for_real_values() {
    let lines = render(&all_open());
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("ISPENDR0=0x8000000"));
    assert!(lines[1].contains("PMR=0xf0"));
    assert!(lines[2].contains("ELRSR=0xffff"));
    assert!(lines[2].contains("lrs=[none]"));
}

#[test]
fn occupied_list_registers_render_with_their_index() {
    let state = GicIrqState {
        ich_lrs: vec![(2, 0xa000_0000_0000_001b)],
        ..all_open()
    };
    assert!(render(&state)[2].contains("lrs=[LR2=0xa00000000000001b]"));
}

#[test]
fn a_busy_rpr_without_a_banked_active_interrupt_stays_ambiguous() {
    let state = GicIrqState {
        // Pending, enabled, group on, PMR open. A zero GICR_ISACTIVER0
        // excludes SGIs/PPIs only; a shared SPI/MSI may still be active.
        rpr: Some(0xa0),
        ..all_open()
    };
    assert_eq!(
        state.vtimer_verdict(),
        Some("vtimer PPI pending; ICC_RPR busy and shared active state unavailable")
    );
}

#[test]
fn a_busy_rpr_with_a_real_banked_active_interrupt_is_not_a_wedge() {
    let state = GicIrqState {
        rpr: Some(0xa0),
        isactiver0: Some(u64::from(VTIMER_PPI_BIT)),
        ..all_open()
    };
    // In-service vtimer PPI: the guest is handling it; the GIC is fine.
    assert_eq!(
        state.vtimer_verdict(),
        Some("vtimer PPI pending and deliverable; the block is not the GIC")
    );
}

#[test]
fn a_failed_rpr_read_fails_the_verdict_rather_than_guessing() {
    let state = GicIrqState { rpr: None, ..all_open() };
    assert_eq!(state.vtimer_verdict(), None);
}

