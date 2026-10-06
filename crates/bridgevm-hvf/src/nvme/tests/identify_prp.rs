//! Identify must honor the two-page PRP layout without writing adjacent RAM.

use super::super::*;
use super::helpers::*;
use super::identify_prp_support::*;
use crate::fwcfg::GuestMemoryMut;

#[test]
fn identify_controller_uses_nonadjacent_prp2() {
    scattered(CASES[0]);
}

#[test]
fn identify_namespace_one_uses_nonadjacent_prp2() {
    scattered(CASES[1]);
}

#[test]
fn identify_namespace_two_uses_nonadjacent_prp2() {
    scattered(CASES[2]);
}

#[test]
fn identify_unallocated_namespace_uses_nonadjacent_prp2() {
    scattered(CASES[3]);
}

#[test]
fn identify_active_namespaces_use_nonadjacent_prp2() {
    scattered(CASES[4]);
}

#[test]
fn identify_namespace_one_descriptors_use_nonadjacent_prp2() {
    scattered(CASES[5]);
}

#[test]
fn identify_namespace_two_descriptors_use_nonadjacent_prp2() {
    scattered(CASES[6]);
}

#[test]
fn identify_command_set_uses_nonadjacent_prp2() {
    scattered(CASES[7]);
}

#[test]
fn identify_namespace_list_after_one_uses_nonadjacent_prp2() {
    scattered(CASES[8]);
}

#[test]
fn identify_aligned_buffer_ignores_unused_prp2() {
    for kind in CASES {
        let data = reference(kind);
        let (mut ctrl, mut mem) = fixture();
        fill_data(&mut mem);
        assert_eq!(
            submit(&mut ctrl, &mut mem, 0, kind, (DATA_BASE, u64::MAX)),
            SC_SUCCESS
        );
        let mut expected = vec![SENTINEL; DATA_LEN];
        expected[..PAGE_SIZE].copy_from_slice(&data);
        assert_eq!(mem.read_bytes(DATA_BASE, DATA_LEN).unwrap(), expected);
    }
}

#[test]
fn identify_missing_second_prp_is_rejected_before_dma() {
    for kind in CASES {
        let (mut ctrl, mut mem) = fixture();
        fill_data(&mut mem);
        assert_eq!(
            submit(&mut ctrl, &mut mem, 0, kind, (DATA_BASE + 4, 0)),
            SC_INVALID_FIELD
        );
        assert_eq!(
            mem.read_bytes(DATA_BASE, DATA_LEN).unwrap(),
            vec![SENTINEL; DATA_LEN]
        );
    }
}

#[test]
fn identify_unaligned_second_prp_is_rejected_before_dma() {
    for kind in CASES {
        let (mut ctrl, mut mem) = fixture();
        fill_data(&mut mem);
        assert_eq!(
            submit(
                &mut ctrl,
                &mut mem,
                0,
                kind,
                (DATA_BASE + 4, SECOND_PAGE + 4)
            ),
            SC_INVALID_FIELD
        );
        assert_eq!(
            mem.read_bytes(DATA_BASE, DATA_LEN).unwrap(),
            vec![SENTINEL; DATA_LEN]
        );
    }
}

#[test]
fn identify_unbacked_second_prp_reports_error_without_adjacent_dma() {
    for kind in CASES {
        let (mut ctrl, mut mem) = fixture();
        fill_data(&mut mem);
        assert_eq!(
            submit(&mut ctrl, &mut mem, 0, kind, (DATA_BASE + 4, UNBACKED)),
            SC_INVALID_FIELD
        );
        // The valid first span may have been written. No rollback is required.
        assert_eq!(
            mem.read_bytes(DATA_BASE + PAGE_SIZE_U64, DATA_LEN - PAGE_SIZE)
                .unwrap(),
            vec![SENTINEL; DATA_LEN - PAGE_SIZE]
        );
    }
}

#[test]
fn identify_unbacked_first_prp_reports_error() {
    let (mut ctrl, mut mem) = fixture();
    fill_data(&mut mem);
    assert_eq!(
        submit(&mut ctrl, &mut mem, 0, CASES[0], (UNBACKED, 0)),
        SC_INVALID_FIELD
    );
    assert_eq!(
        mem.read_bytes(DATA_BASE, DATA_LEN).unwrap(),
        vec![SENTINEL; DATA_LEN]
    );
}
