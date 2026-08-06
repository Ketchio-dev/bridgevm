use super::*;

fn all_open() -> GicIrqState {
    GicIrqState {
        isenabler0: Some(u64::from(VTIMER_PPI_BIT)),
        ispendr0: Some(u64::from(VTIMER_PPI_BIT)),
        isactiver0: Some(0),
        pmr: Some(0xf0),
        igrpen1: Some(1),
        ctlr: Some(0),
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
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("ISPENDR0=0x8000000"));
    assert!(lines[1].contains("PMR=0xf0"));
}
