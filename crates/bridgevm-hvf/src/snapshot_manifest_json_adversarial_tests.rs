use crate::snapshot_pair::{SnapshotError, SnapshotManifest, SNAPSHOT_FORMAT_VERSION};

fn manifest() -> SnapshotManifest {
    SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: "vm".into(),
        disk_bytes: 10,
        disk_sha256: "a".repeat(64),
        vars_bytes: 20,
        vars_sha256: "b".repeat(64),
    }
}

fn assert_bad(text: &str) {
    assert!(
        matches!(
            SnapshotManifest::from_json(text),
            Err(SnapshotError::BadManifest(_))
        ),
        "accepted ambiguous or malformed manifest: {text:?}"
    );
}

#[test]
fn every_control_character_and_unicode_vm_id_round_trips() {
    let mut expected = manifest();
    expected.vm_id = (0u8..=31).map(char::from).collect::<String>() + "\"\\ 한글 🦀";
    assert_eq!(
        SnapshotManifest::from_json(&expected.to_json()).unwrap(),
        expected
    );
}

#[test]
fn standard_json_string_escapes_are_decoded() {
    let mut expected = manifest();
    let text = expected.to_json().replace(
        "\"vm_id\": \"vm\"",
        r#""vm_id": "\b\f\n\r\t\/\u0041\ud83e\udd80""#,
    );
    expected.vm_id = "\u{8}\u{c}\n\r\t/A🦀".into();
    assert_eq!(SnapshotManifest::from_json(&text).unwrap(), expected);
}

#[test]
fn malformed_and_trailing_documents_are_refused() {
    let text = manifest().to_json();
    for bad in [
        text.trim().trim_start_matches('{').to_string(),
        text.trim().trim_end_matches('}').to_string(),
        text.replace(",\n", "\n"),
        text.clone() + "garbage",
        text.clone() + "{}",
        format!("[{text}]"),
        format!(
            "[1,\"vm\",10,\"{}\",20,\"{}\"]",
            "a".repeat(64),
            "b".repeat(64)
        ),
    ] {
        assert_bad(&bad);
    }
}

#[test]
fn integer_fields_refuse_suffixes_and_non_integer_types() {
    let text = manifest().to_json();
    for (field, original) in [
        ("format_version", 1),
        ("disk_bytes", 10),
        ("vars_bytes", 20),
    ] {
        for value in [
            "1e2",
            "1.0",
            "1suffix",
            "-1",
            "+1",
            "01",
            "null",
            "\"1\"",
            "true",
            "18446744073709551616",
        ] {
            assert_bad(&text.replace(
                &format!("\"{field}\": {original}"),
                &format!("\"{field}\": {value}"),
            ));
        }
    }
    let mut maximum = manifest();
    maximum.disk_bytes = u64::MAX;
    maximum.vars_bytes = u64::MAX;
    assert_eq!(
        SnapshotManifest::from_json(&maximum.to_json()).unwrap(),
        maximum
    );
}

#[test]
fn every_duplicate_known_field_is_refused() {
    let text = manifest().to_json();
    let fields: serde_json::Value = serde_json::from_str(&text).unwrap();
    for (key, value) in fields.as_object().unwrap() {
        assert_bad(&text.replacen('{', &format!("{{\"{key}\":{value},"), 1));
    }
    assert_bad(&text.replacen('{', r#"{"\u0076m_id":"vm","#, 1));
}

#[test]
fn fields_must_be_top_level_with_the_declared_types() {
    let fields: serde_json::Value = serde_json::from_str(&manifest().to_json()).unwrap();
    for (key, value) in fields.as_object().unwrap() {
        let mut nested = fields.clone();
        nested.as_object_mut().unwrap().remove(key);
        nested["extension"] = serde_json::json!({key: value});
        assert_bad(&nested.to_string());
        let mut wrong_type = fields.clone();
        wrong_type[key] = if value.is_string() {
            serde_json::json!(1)
        } else {
            serde_json::json!("1")
        };
        assert_bad(&wrong_type.to_string());
    }
}

#[test]
fn invalid_json_string_escapes_and_raw_controls_are_refused() {
    let text = manifest().to_json();
    for value in [
        r#""\q""#,
        r#""\u000x""#,
        r#""\ud800""#,
        r#""\udc00""#,
        "\"raw\nnewline\"",
        "\"raw\tcontrol\"",
    ] {
        assert_bad(&text.replace("\"vm_id\": \"vm\"", &format!("\"vm_id\": {value}")));
    }
}

#[test]
fn additional_fields_cannot_shadow_the_known_fields() {
    let expected = manifest();
    let text = expected.to_json().replacen(
        '{',
        r#"{"extension":{"format_version":999,"vm_id":"other","disk_bytes":0},"#,
        1,
    );
    assert_eq!(SnapshotManifest::from_json(&text).unwrap(), expected);
}
