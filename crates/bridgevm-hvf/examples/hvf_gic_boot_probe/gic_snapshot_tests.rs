use super::*;

/// A healthy vCPU waiting on a deadline that has not arrived.
fn waiting() -> GicSnapshot {
    GicSnapshot {
        mpidr: 0x8000_0000,
        pc: 0x4000_008c,
        cntv_ctl: 0x1, // ENABLE=1, IMASK=0, ISTATUS=0
        cntv_cval: 2_000,
        guest_now: 1_000,
        vtimer_masked: false,
        psci_state: 2,
        generation: 1,
    }
}

/// The A1 stall fingerprint measured by the T1 microprobe.
fn parked_unreachable() -> GicSnapshot {
    GicSnapshot {
        cntv_ctl: 0x5, // ENABLE=1, ISTATUS=1
        cntv_cval: 1_000,
        guest_now: 2_000,
        vtimer_masked: true,
        ..waiting()
    }
}

#[test]
fn control_bits_are_decoded_the_way_the_architecture_defines_them() {
    let snapshot = GicSnapshot {
        cntv_ctl: 0x5,
        ..waiting()
    };
    assert!(snapshot.timer_enabled());
    assert!(!snapshot.guest_masked());
    assert!(snapshot.condition_met());

    let masked = GicSnapshot {
        cntv_ctl: 0x3,
        ..waiting()
    };
    assert!(masked.timer_enabled());
    assert!(masked.guest_masked());
    assert!(!masked.condition_met());
}

#[test]
fn a_future_deadline_is_not_overdue() {
    assert_eq!(waiting().overdue_ticks(), 0);
}

#[test]
fn a_passed_deadline_reports_how_far_it_slipped() {
    assert_eq!(parked_unreachable().overdue_ticks(), 1_000);
}

#[test]
fn a_masked_vtimer_with_a_passed_deadline_cannot_deliver_its_wake() {
    assert!(parked_unreachable().wake_is_unreachable());
}

#[test]
fn a_guest_masked_timer_with_the_condition_met_is_also_unreachable() {
    let snapshot = GicSnapshot {
        cntv_ctl: 0x7, // ENABLE, IMASK, ISTATUS
        vtimer_masked: false,
        ..parked_unreachable()
    };
    assert!(snapshot.wake_is_unreachable());
}

#[test]
fn an_unmasked_overdue_timer_can_still_deliver() {
    let snapshot = GicSnapshot {
        vtimer_masked: false,
        cntv_ctl: 0x1,
        ..parked_unreachable()
    };
    assert!(!snapshot.wake_is_unreachable());
}

#[test]
fn a_disabled_timer_is_never_called_unreachable() {
    // A guest that turned its timer off is not waiting on it, so reporting an
    // unreachable wake would be a false positive.
    let snapshot = GicSnapshot {
        cntv_ctl: 0,
        vtimer_masked: true,
        ..parked_unreachable()
    };
    assert!(!snapshot.wake_is_unreachable());
}

#[test]
fn a_moving_pc_means_the_vcpu_is_running() {
    let before = waiting();
    let after = GicSnapshot {
        pc: before.pc + 4,
        ..before
    };
    assert_eq!(compare(&before, &after), ProgressVerdict::Running);
}

#[test]
fn a_re_armed_deadline_means_the_vcpu_is_running_even_at_the_same_pc() {
    // The idle loop parks at one instruction, so the PC alone would suggest a
    // stall; a moved CVAL proves the guest went round the loop.
    let before = parked_unreachable();
    let after = GicSnapshot {
        cntv_cval: before.cntv_cval + 5_000,
        ..before
    };
    assert_eq!(compare(&before, &after), ProgressVerdict::Running);
}

#[test]
fn an_unchanged_pc_with_a_future_deadline_is_healthy_waiting() {
    let snapshot = waiting();
    assert_eq!(
        compare(&snapshot, &snapshot),
        ProgressVerdict::WaitingForFutureDeadline
    );
    assert!(!compare(&snapshot, &snapshot).is_stall());
}

#[test]
fn a_parked_vcpu_with_a_pending_masked_timer_is_the_stall_fingerprint() {
    let snapshot = parked_unreachable();
    let verdict = compare(&snapshot, &snapshot);
    assert_eq!(verdict, ProgressVerdict::ParkedWithUnreachableWake);
    assert!(verdict.is_stall());
}

#[test]
fn a_parked_vcpu_whose_wake_could_still_arrive_is_reported_separately() {
    // This is the a1-fix3/boot-7 shape: overdue but masked=false. It is a
    // different problem and must not be collapsed into the masked case.
    let snapshot = GicSnapshot {
        vtimer_masked: false,
        cntv_ctl: 0x1,
        ..parked_unreachable()
    };
    let verdict = compare(&snapshot, &snapshot);
    assert_eq!(verdict, ProgressVerdict::ParkedWithDeliverableWake);
    assert!(
        !verdict.is_stall(),
        "only the unreachable-wake case is the defect this gate names"
    );
}

#[test]
fn snapshots_from_different_generations_are_refused() {
    let before = waiting();
    let after = GicSnapshot {
        generation: before.generation + 1,
        ..before
    };
    assert_eq!(compare(&before, &after), ProgressVerdict::GenerationChanged);
}

#[test]
fn the_rendered_report_is_bounded_and_names_the_verdict() {
    let snapshot = parked_unreachable();
    let lines = render(&snapshot, &snapshot);
    assert_eq!(lines.len(), 3, "the report must stay bounded");
    assert!(lines[0].contains("parked (deadline passed, wake unreachable)"));
    assert!(lines[2].contains("overdue_ticks=1000"));
    assert!(lines[2].contains("istatus=true"));
}

#[test]
fn every_verdict_has_a_name() {
    for verdict in [
        ProgressVerdict::Running,
        ProgressVerdict::WaitingForFutureDeadline,
        ProgressVerdict::ParkedWithUnreachableWake,
        ProgressVerdict::ParkedWithDeliverableWake,
        ProgressVerdict::GenerationChanged,
    ] {
        assert!(!verdict.as_str().is_empty());
    }
}
