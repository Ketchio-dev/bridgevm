use super::*;

#[test]
fn replacement_members_cannot_inherit_the_original_seal() {
    for name in ["disk.raw", "vars.fd", "manifest.json"] {
        let fixture = Fixture::new(&format!("owned-seal-member-{name}"));
        fixture.seal();
        let path = fixture.stage.join(name);
        let copied = fixture.scratch.path("replacement");
        fs::copy(&path, &copied).unwrap();
        fs::rename(&copied, &path).unwrap();
        let record = Record::read_pinned(&fixture.root, &fixture.stage).unwrap();
        assert!(record.matches_stage(&fixture.stage).is_err());
        fixture.assert_media();
    }
}

#[test]
fn copied_record_and_pin_do_not_acquire_the_record_self_identity() {
    let fixture = Fixture::new("owned-seal-copied-record");
    fixture.seal();
    let root_record = fixture.root.join(ROOT_NAME);
    let copy = fixture.scratch.path("copied-record");
    fs::copy(&root_record, &copy).unwrap();
    fs::remove_file(fixture.stage.join(PIN_NAME)).unwrap();
    fs::rename(&copy, &root_record).unwrap();
    fs::hard_link(&root_record, fixture.stage.join(PIN_NAME)).unwrap();
    assert!(Record::read_pinned(&fixture.root, &fixture.stage).is_err());
    fixture.assert_media();
}

#[test]
fn current_pin_remains_readable_after_writes_change_media_lengths() {
    let fixture = Fixture::new("owned-seal-current-writes");
    fixture.seal();
    let current = fixture.root.join("current");
    fs::rename(&fixture.stage, &current).unwrap();
    fs::write(current.join("disk.raw"), b"guest writes a different length").unwrap();
    let record = Record::read_pinned(&fixture.root, &current).unwrap();
    assert!(record.stage.matches(&current).unwrap());
    assert!(record.matches_stage(&current).is_err());
}

#[test]
fn malformed_or_unbounded_records_never_authorize_recovery() {
    for scenario in [
        "unknown",
        "duplicate",
        "version",
        "array",
        "nested-array",
        "oversize",
    ] {
        let fixture = Fixture::new(&format!("owned-seal-schema-{scenario}"));
        fixture.seal();
        let path = fixture.root.join(ROOT_NAME);
        let text = fs::read_to_string(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let replacement = match scenario {
            "unknown" => {
                value["unknown"] = true.into();
                value.to_string()
            }
            "duplicate" => text.replacen('{', "{\"version\":1,", 1),
            "version" => {
                value["version"] = 2.into();
                value.to_string()
            }
            "array" => "[]".into(),
            "nested-array" => {
                value["root"] = serde_json::json!([value["root"]["dev"], value["root"]["ino"]]);
                value.to_string()
            }
            "oversize" => " ".repeat(256 * 1024 + 1),
            _ => unreachable!(),
        };
        fs::write(&path, replacement).unwrap();
        assert!(
            Record::read_pinned(&fixture.root, &fixture.stage).is_err(),
            "{scenario}"
        );
        fixture.assert_media();
    }
}

#[test]
fn valid_near_limit_manifest_survives_canonical_nested_escaping() {
    let mut fixture = Fixture::new("owned-seal-large-valid-manifest");
    fixture.manifest.vm_id = "\n".repeat(32_000);
    let canonical = fixture.manifest.to_json();
    let compact: serde_json::Value = serde_json::from_str(&canonical).unwrap();
    let source_text = compact.to_string();
    assert!(source_text.len() < 64 * 1024);
    fs::write(fixture.stage.join("manifest.json"), source_text).unwrap();
    let record = fixture.seal();
    assert!(record.encode().unwrap().len() > 200 * 1024);
    assert_eq!(
        Record::read_pinned(&fixture.root, &fixture.stage)
            .unwrap()
            .expected_manifest()
            .unwrap(),
        fixture.manifest
    );
    fixture.assert_media();
}
