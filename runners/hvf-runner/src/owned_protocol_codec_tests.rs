use super::super::dto::Event;
use super::*;
use std::path::Path;
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/owned-runtime-v1")
            .join(name),
    )
    .unwrap()
}
#[test]
fn shared_golden_frames_have_exact_canonical_bytes() {
    for name in [
        "ready.json",
        "stop-ack.json",
        "swtpm-started.json",
        "helper-started.json",
        "helper-reaped.json",
        "swtpm-reaped.json",
        "cleanup-unconfirmed.json",
        "complete.json",
        "complete-not-admitted.json",
    ] {
        let bytes = fixture(name);
        let value: Event = decode(&bytes).unwrap();
        assert_eq!(encode(&value).unwrap(), bytes);
    }
    for (name, key) in [
        ("hello-no-key.json", false),
        ("hello-synthetic-key.json", true),
    ] {
        let bytes = fixture(name);
        let mut value = hello(&bytes, &"a".repeat(64), key).unwrap();
        let secret = take_key(&mut value);
        assert_eq!(secret.is_some(), key);
        if let Some(secret) = secret {
            assert_eq!(secret, (0..32).collect::<Vec<u8>>());
        }
    }
    assert!(stop(
        &fixture("stop.json"),
        "12345678-1234-4234-8234-123456789abc"
    )
    .is_ok());
}
#[test]
fn invalid_secret_or_noncanonical_owner_frame_is_rejected() {
    let text = String::from_utf8(fixture("hello-no-key.json")).unwrap();
    for malformed in [
        text.replace(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
        ),
        text.replace("\"keyHex\":null,", ""),
        text.replace("\"keyHex\":null", "\"keyHex\":\"00\""),
        text.replace("\"sequence\":0", "\"sequence\":0.0"),
        format!("{text}\n"),
        text.replace("\"sequence\":0", "\"sequence\":0,\"unknown\":null"),
    ] {
        assert!(hello(malformed.as_bytes(), &"a".repeat(64), false).is_err());
    }
    assert!(hello(text.as_bytes(), &"b".repeat(64), false).is_err());
    assert!(hello(text.as_bytes(), &"a".repeat(64), true).is_err());
    assert!(!uuid("12345678-1234-4234-8234-123456789ABc"));
    assert!(decode::<Hello>(&vec![b' '; MAX_FRAME + 1]).is_err());
}
