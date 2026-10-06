use super::*;

#[test]
fn runner_uses_the_approved_builder_digest() {
    let approved = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/firmware/bridgevm-pc-dxe-entry.sha256"
    ));
    assert_eq!(approved_firmware::expected().unwrap(), approved.trim_end());
}

#[test]
fn rejects_unknown_correct_size_firmware() {
    let bytes = vec![0; board::FLASH_CODE.size as usize];
    let error = validate(&bytes).unwrap_err();
    assert!(error.contains("FD digest"));
    assert!(error.contains(approved_firmware::expected().unwrap()));
}

#[test]
fn refuses_malformed_approved_pins() {
    for value in [
        "",
        "a",
        &"a".repeat(64),
        &format!("{}\n\n", "a".repeat(64)),
        &format!("{}\n", "A".repeat(64)),
        &format!("{}\n", "g".repeat(64)),
        &format!("{}\n", "é".repeat(32)),
    ] {
        assert!(
            approved_firmware::parse(value).is_err(),
            "accepted {value:?}"
        );
    }
}
