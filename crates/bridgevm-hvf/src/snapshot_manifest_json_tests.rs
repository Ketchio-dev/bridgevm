use super::super::*;

#[test]
fn a_manifest_round_trips_through_json() {
    let m = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: "windows-11".into(),
        disk_bytes: 68_719_476_736,
        disk_sha256: "a".repeat(64),
        vars_bytes: 67_108_864,
        vars_sha256: "b".repeat(64),
    };
    assert_eq!(SnapshotManifest::from_json(&m.to_json()).unwrap(), m);
}

#[test]
fn a_vm_id_with_quotes_does_not_break_the_manifest() {
    let m = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: r#"vm "quoted" \ backslash"#.into(),
        disk_bytes: 1,
        disk_sha256: "c".repeat(64),
        vars_bytes: 2,
        vars_sha256: "d".repeat(64),
    };
    assert_eq!(SnapshotManifest::from_json(&m.to_json()).unwrap(), m);
}
