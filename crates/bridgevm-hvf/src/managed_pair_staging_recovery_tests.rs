use super::super::record::PIN_NAME;
use super::*;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch, snapshot_publish};
use std::os::unix::fs::PermissionsExt;

#[path = "managed_pair_staging_recovery_test_fixture.rs"]
mod fixture;
use fixture::{contents, Fixture};

#[test]
fn sealed_own_stage_allows_ordinary_initial_and_swap_retry() {
    for selected in [false, true] {
        let fixture = Fixture::new(&format!("owned-retry-{selected}"), selected, true);
        fixture.assert_inputs();
        let mut pair = fixture.pair();
        assert_eq!(pair.restore(&fixture.snapshot).unwrap(), fixture.manifest);
        let current = pair.paths().unwrap();
        assert_eq!(fs::read(&current.0).unwrap(), fixture.source[0]);
        assert_eq!(fs::read(&current.1).unwrap(), fixture.source[1]);
        assert_eq!(contents(&fixture.snapshot), fixture.source);
        assert!(!fixture.stage.exists());
        assert!(!fixture.root.join(ROOT_NAME).exists());
    }
}

#[test]
fn authenticated_old_stage_does_not_require_the_same_requested_snapshot() {
    let fixture = Fixture::new("owned-retry-different-source", true, true);
    let inputs = Scratch::new("owned-retry-different-inputs");
    let disk = inputs.write("different-disk", b"different snapshot disk");
    let vars = inputs.write("different-vars", b"different snapshot vars");
    let source = fixture.scratch.path("different");
    let manifest = create_snapshot(&disk, &vars, &source, "different", false, 1024).unwrap();
    let before = contents(&source);
    let mut pair = fixture.pair();
    assert_eq!(pair.restore(&source).unwrap(), manifest);
    let current = pair.paths().unwrap();
    assert_eq!(fs::read(current.0).unwrap(), before[0]);
    assert_eq!(fs::read(current.1).unwrap(), before[1]);
    assert_eq!(contents(&source), before);
    assert_eq!(contents(&fixture.snapshot), fixture.source);
}

#[test]
fn legacy_complete_stage_and_real_foreign_export_stay_intact() {
    for foreign in [false, true] {
        let fixture = Fixture::new(&format!("owned-retry-unsealed-{foreign}"), true, false);
        if foreign {
            let inputs = Scratch::new("owned-retry-foreign-inputs");
            let disk = inputs.write("foreign-disk", b"foreign disk");
            let vars = inputs.write("foreign-vars", b"foreign vars");
            create_snapshot(&disk, &vars, &fixture.stage, "foreign-vm", false, 1024).unwrap();
            // Equivalent private admission to the actual CLI umask077 recipe,
            // without changing the process-wide umask of parallel tests.
            fs::set_permissions(&fixture.stage, fs::Permissions::from_mode(0o700)).unwrap();
        } else {
            fixture.complete_unsealed();
        }
        fixture.assert_refused();
    }
}

#[test]
fn current_record_retirement_tolerates_guest_writes_without_touching_media() {
    let fixture = Fixture::new("owned-current-retirement", false, true);
    let current = fixture.root.join("current");
    snapshot_publish::publish(&fixture.stage, &current).unwrap();
    fs::write(
        current.join("disk.raw"),
        b"new writable current disk with changed length",
    )
    .unwrap();
    fs::write(current.join("vars.fd"), b"new writable vars").unwrap();
    let before = contents(&current);
    assert!(reclaim(&fixture.root).unwrap());
    assert_eq!(contents(&current), before);
    assert!(!fixture.root.join(ROOT_NAME).exists());
    assert!(current.join(PIN_NAME).is_file());
    let mut pair = fixture.pair();
    pair.restore(&fixture.snapshot).unwrap();
    assert_eq!(
        fs::read(pair.paths().unwrap().0).unwrap(),
        fixture.source[0]
    );
}

#[path = "managed_pair_staging_recovery_refusal_tests.rs"]
mod refusal_tests;

#[path = "managed_pair_staging_runtime_tests.rs"]
mod runtime_tests;

#[path = "managed_pair_staging_legacy_exchange_tests.rs"]
mod legacy_exchange_tests;

#[path = "managed_pair_staging_retirement_tests.rs"]
mod retirement_tests;
