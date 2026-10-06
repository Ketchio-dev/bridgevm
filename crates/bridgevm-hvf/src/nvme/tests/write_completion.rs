//! Guest WRITE completion after VWC disable and with FUA, using backend sync counts.

use super::super::*;
use super::helpers::*;
use super::write_completion_support::WriteFixture;

fn completion_case(cached: bool, fua: bool, direct: bool, nsid: u32, fails: bool) {
    let mut fixture = WriteFixture::new(direct);
    if !cached {
        fixture.cache(2, false);
    }
    let before = fixture.counters();
    if fails {
        fixture.fail_sync(nsid);
    }
    let status = fixture.write(nsid, fua, DATA_BASE, 0);
    assert_eq!(
        status,
        if fails {
            SC_INTERNAL_DEVICE_ERROR
        } else {
            SC_SUCCESS
        }
    );
    let mut expected = before;
    expected[(nsid - 1) as usize] += 1;
    assert_eq!(
        fixture.counters(),
        expected,
        "sync only the written namespace before completion"
    );
    // A sync error must not promise rollback: data transfer already occurred.
    fixture.assert_bytes(nsid, true);
}

macro_rules! write_case {
    ($name:ident, $cached:expr, $fua:expr, $direct:expr, $nsid:expr, $fails:expr) => {
        #[test]
        fn $name() {
            completion_case($cached, $fua, $direct, $nsid, $fails);
        }
    };
}

write_case!(
    vwc_buffered_primary_success,
    false,
    false,
    false,
    NSID,
    false
);
write_case!(
    vwc_buffered_secondary_success,
    false,
    false,
    false,
    NSID2,
    false
);
write_case!(vwc_direct_primary_success, false, false, true, NSID, false);
write_case!(
    vwc_direct_secondary_success,
    false,
    false,
    true,
    NSID2,
    false
);
write_case!(
    vwc_buffered_primary_failure,
    false,
    false,
    false,
    NSID,
    true
);
write_case!(
    vwc_buffered_secondary_failure,
    false,
    false,
    false,
    NSID2,
    true
);
write_case!(vwc_direct_primary_failure, false, false, true, NSID, true);
write_case!(
    vwc_direct_secondary_failure,
    false,
    false,
    true,
    NSID2,
    true
);
write_case!(fua_buffered_primary_success, true, true, false, NSID, false);
write_case!(
    fua_buffered_secondary_success,
    true,
    true,
    false,
    NSID2,
    false
);
write_case!(fua_direct_primary_success, true, true, true, NSID, false);
write_case!(fua_direct_secondary_success, true, true, true, NSID2, false);
write_case!(fua_buffered_primary_failure, true, true, false, NSID, true);
write_case!(
    fua_buffered_secondary_failure,
    true,
    true,
    false,
    NSID2,
    true
);
write_case!(fua_direct_primary_failure, true, true, true, NSID, true);
write_case!(fua_direct_secondary_failure, true, true, true, NSID2, true);

#[test]
fn ordinary_cached_and_reenabled_writes_do_not_sync() {
    for direct in [false, true] {
        for nsid in [NSID, NSID2] {
            for reenable in [false, true] {
                let mut fixture = WriteFixture::new(direct);
                if reenable {
                    fixture.cache(2, false);
                    fixture.cache(3, true);
                }
                let before = fixture.counters();
                fixture.fail_sync(nsid);
                assert_eq!(fixture.write(nsid, false, DATA_BASE, 0), SC_SUCCESS);
                assert_eq!(fixture.counters(), before);
                fixture.assert_bytes(nsid, true);
            }
        }
    }
}

#[test]
fn restored_disabled_cache_still_syncs_subsequent_write() {
    for direct in [false, true] {
        let mut fixture = WriteFixture::new(direct);
        fixture.cache(2, false);
        let snapshot = fixture.ctrl.snapshot_state();
        fixture.ctrl.restore_state(&snapshot);
        let before = fixture.counters();
        assert_eq!(fixture.write(NSID2, false, DATA_BASE, 0), SC_SUCCESS);
        assert_eq!(fixture.counters(), [before[0], before[1] + 1]);
        fixture.assert_bytes(NSID2, true);
    }
}

#[test]
fn failed_or_invalid_writes_keep_status_and_do_not_sync() {
    for direct in [false, true] {
        for nsid in [NSID, NSID2] {
            for cached in [false, true] {
                for failure in 0..3 {
                    let mut fixture = WriteFixture::new(direct);
                    if !cached {
                        fixture.cache(2, false);
                    }
                    let before = fixture.counters();
                    fixture.fail_sync(nsid);
                    let (prp, lba) = match failure {
                        0 => (DATA_BASE, 8),          // First LBA outside the namespace.
                        1 => (MEM_BASE + 0x10000, 0), // Unbacked source memory.
                        _ => {
                            fixture.fail_write(nsid);
                            (DATA_BASE, 0)
                        }
                    };
                    assert_eq!(fixture.write(nsid, cached, prp, lba), SC_INVALID_FIELD);
                    assert_eq!(fixture.counters(), before);
                    fixture.assert_bytes(nsid, false);
                }
            }
        }
    }
}
