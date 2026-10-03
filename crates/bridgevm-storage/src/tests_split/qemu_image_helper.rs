//! Fixed-path policy refusals complement the actual release CLI contract.

use super::helpers::temp_store;
use crate::qemu_image_helper::resolve_release;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;

#[test]
fn release_helper_refuses_missing_unapproved_and_nonexecutable_candidates() {
    let store = temp_store();
    fs::create_dir_all(store.root()).unwrap();
    let missing = store.root().join("missing");
    let present = store.root().join("qemu-img");
    fs::write(&present, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&present, fs::Permissions::from_mode(0o600)).unwrap();
    let candidates = [missing.to_str().unwrap(), present.to_str().unwrap()];
    assert_eq!(
        resolve_release("qemu-img", &candidates).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    fs::set_permissions(&present, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(resolve_release("qemu-img", &candidates).unwrap(), present);
    for program in ["/tmp/qemu-img", "./qemu-img", "qemu-system-aarch64", ""] {
        assert_eq!(
            resolve_release(program, &candidates).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    fs::remove_dir_all(store.root()).unwrap();
}
