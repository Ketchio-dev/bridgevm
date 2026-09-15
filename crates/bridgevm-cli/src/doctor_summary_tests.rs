use super::*;

fn summary(statuses: &[DoctorCheckStatus]) -> String {
    environment_summary(
        &statuses
            .iter()
            .map(|status| DoctorCheck {
                status: *status,
                name: "fixture check".into(),
                detail: "fixture evidence".into(),
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn all_successful_checks_report_counted_ok() {
    assert_eq!(
        summary(&[DoctorCheckStatus::Ok; 3]),
        "Client environment summary: OK (OK: 3, WARN: 0, MISSING: 0)"
    );
}

#[test]
fn warnings_cannot_be_summarized_as_ok() {
    assert_eq!(
        summary(&[DoctorCheckStatus::Ok, DoctorCheckStatus::Warn]),
        "Client environment summary: WARN (OK: 1, WARN: 1, MISSING: 0)"
    );
}

#[test]
fn missing_checks_remain_visible_alongside_warnings() {
    assert_eq!(
        summary(&[
            DoctorCheckStatus::Warn,
            DoctorCheckStatus::Missing,
            DoctorCheckStatus::Ok,
            DoctorCheckStatus::Missing,
        ]),
        "Client environment summary: MISSING (OK: 1, WARN: 1, MISSING: 2)"
    );
}

#[test]
fn empty_audit_does_not_claim_success() {
    assert_eq!(
        summary(&[]),
        "Client environment summary: NOT CHECKED (OK: 0, WARN: 0, MISSING: 0)"
    );
}
