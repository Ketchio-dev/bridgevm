//! PRP placement for existing Get Log Page and Security Receive responses.

use super::super::*;
use super::admin_prp_support::*;
use super::helpers::*;

#[test]
fn smart_log_uses_nonadjacent_second_prp() {
    scattered(CASES[0]);
}
#[test]
fn firmware_log_uses_nonadjacent_second_prp() {
    scattered(CASES[1]);
}
#[test]
fn command_effects_log_uses_nonadjacent_second_prp() {
    scattered(CASES[2]);
}
#[test]
fn security_discovery_uses_nonadjacent_second_prp() {
    scattered(CASES[3]);
}

#[test]
fn aligned_existing_responses_ignore_unused_second_prp() {
    for case in CASES {
        let data = reference(case);
        let (mut ctrl, mut mem) = fixture();
        assert_eq!(
            submit(&mut ctrl, &mut mem, case, (DATA_BASE, u64::MAX)),
            SC_SUCCESS
        );
        let mut expected = vec![SENTINEL; DATA_LEN];
        expected[..case.len].copy_from_slice(&data);
        assert_data_region(&mem, &expected);
    }
}

#[test]
fn offset_responses_fitting_one_page_ignore_unused_second_prp() {
    for case in [CASES[0], CASES[1], CASES[3]] {
        let data = reference(case);
        let offset = (PAGE_SIZE - case.len) & !3;
        let (mut ctrl, mut mem) = fixture();
        assert_eq!(
            submit(
                &mut ctrl,
                &mut mem,
                case,
                (DATA_BASE + offset as u64, u64::MAX)
            ),
            SC_SUCCESS
        );
        let mut expected = vec![SENTINEL; DATA_LEN];
        expected[offset..offset + case.len].copy_from_slice(&data);
        assert_data_region(&mem, &expected);
    }
}

#[test]
fn four_byte_log_request_writes_only_requested_prefix() {
    let (mut ctrl, mut mem) = fixture();
    let case = Case { len: 4, ..CASES[0] };
    assert_eq!(
        submit(
            &mut ctrl,
            &mut mem,
            case,
            (DATA_BASE + PAGE_SIZE_U64 - 4, u64::MAX)
        ),
        SC_SUCCESS
    );
    let mut expected = vec![SENTINEL; DATA_LEN];
    expected[PAGE_SIZE - 4..PAGE_SIZE].copy_from_slice(&[0, 44, 1, 100]);
    assert_data_region(&mem, &expected);
}

#[test]
fn unsupported_logs_fail_before_dma() {
    for selector in [0xc0, 0xc1] {
        let (mut ctrl, mut mem) = fixture();
        let case = Case {
            selector,
            ..CASES[0]
        };
        assert_eq!(
            submit(
                &mut ctrl,
                &mut mem,
                case,
                (DATA_BASE + PAGE_SIZE_U64 - 4, SECOND_PAGE)
            ),
            SC_INVALID_FIELD_DNR
        );
        assert_data_region(&mem, &[SENTINEL; DATA_LEN]);
    }
}

#[test]
fn unsupported_or_short_security_requests_fail_before_dma() {
    for (selector, len) in [(0, 9), (0, 0), (1 << 8, 10), (0xe8 << 24, 10)] {
        let (mut ctrl, mut mem) = fixture();
        let case = Case {
            selector,
            len,
            ..CASES[3]
        };
        assert_eq!(
            submit(
                &mut ctrl,
                &mut mem,
                case,
                (DATA_BASE + PAGE_SIZE_U64 - 4, SECOND_PAGE)
            ),
            SC_INVALID_FIELD_DNR
        );
        assert_data_region(&mem, &[SENTINEL; DATA_LEN]);
    }
}

#[test]
fn logs_missing_second_prp_reports_error() {
    failed_second(&CASES[..3], 0, true);
}

#[test]
fn logs_unaligned_second_prp_reports_error() {
    failed_second(&CASES[..3], SECOND_PAGE + 4, true);
}

#[test]
fn logs_unbacked_second_prp_reports_error() {
    failed_second(&CASES[..3], UNBACKED, false);
}

#[test]
fn logs_unbacked_first_prp_reports_error() {
    for &case in &CASES[..3] {
        let (mut ctrl, mut mem) = fixture();
        assert_eq!(
            submit(&mut ctrl, &mut mem, case, (UNBACKED, 0)),
            SC_INVALID_FIELD
        );
        assert_data_region(&mem, &[SENTINEL; DATA_LEN]);
    }
}

#[test]
fn security_missing_second_prp_reports_error() {
    failed_second(&CASES[3..], 0, true);
}

#[test]
fn security_unaligned_second_prp_reports_error() {
    failed_second(&CASES[3..], SECOND_PAGE + 4, true);
}

#[test]
fn security_unbacked_second_prp_reports_error() {
    failed_second(&CASES[3..], UNBACKED, false);
}

#[test]
fn security_unbacked_first_prp_reports_error() {
    for &case in &CASES[3..] {
        let (mut ctrl, mut mem) = fixture();
        assert_eq!(
            submit(&mut ctrl, &mut mem, case, (UNBACKED, 0)),
            SC_INVALID_FIELD
        );
        assert_data_region(&mem, &[SENTINEL; DATA_LEN]);
    }
}
