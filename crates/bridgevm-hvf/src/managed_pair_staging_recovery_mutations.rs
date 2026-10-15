use super::*;
use std::{
    ffi::CString,
    os::unix::{ffi::OsStrExt, fs::symlink},
};

pub(super) const SCENARIOS: &[&str] = &[
    "missing-record",
    "missing-pin",
    "torn-record",
    "copied-record",
    "copied-pin",
    "copied-both",
    "replaced-stage",
    "replaced-disk",
    "replaced-vars",
    "replaced-manifest",
    "changed-disk",
    "changed-manifest",
    "unknown-entry",
    "missing-member",
    "record-symlink",
    "pin-symlink",
    "disk-symlink",
    "record-fifo",
    "pin-fifo",
    "disk-fifo",
];

fn replace_copy(path: &Path, temporary: &Path) {
    fs::copy(path, temporary).unwrap();
    fs::rename(temporary, path).unwrap();
}

fn fifo(path: &Path) {
    fs::remove_file(path).unwrap();
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: the CString is live and NUL-terminated; only this test owns its
    // tiny synthetic directory. A read must reject this FIFO without blocking.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
}

pub(super) fn apply(fixture: &Fixture, scenario: &str) {
    let record = fixture.root.join(ROOT_NAME);
    let pin = fixture.stage.join(PIN_NAME);
    let temporary = fixture.scratch.path("replacement");
    let retained = fixture.scratch.path("retained-original");
    match scenario {
        "missing-record" => fs::remove_file(record).unwrap(),
        "missing-pin" => fs::remove_file(pin).unwrap(),
        "torn-record" => fs::write(record, b"{").unwrap(),
        "copied-record" => replace_copy(&record, &temporary),
        "copied-pin" => replace_copy(&pin, &temporary),
        "copied-both" => {
            replace_copy(&record, &temporary);
            fs::remove_file(&pin).unwrap();
            fs::hard_link(&record, &pin).unwrap();
        }
        "replaced-stage" => {
            fs::rename(&fixture.stage, &retained).unwrap();
            private_directory(&fixture.stage, true).unwrap();
            for name in ["disk.raw", "vars.fd", "manifest.json"] {
                fs::copy(retained.join(name), fixture.stage.join(name)).unwrap();
            }
            // Keep the genuine two-link token, so directory identity is the
            // rejecting boundary rather than an incidental link-count error.
            fs::rename(retained.join(PIN_NAME), &pin).unwrap();
        }
        "replaced-disk" => replace_copy(&fixture.stage.join("disk.raw"), &temporary),
        "replaced-vars" => replace_copy(&fixture.stage.join("vars.fd"), &temporary),
        "replaced-manifest" => replace_copy(&fixture.stage.join("manifest.json"), &temporary),
        "changed-disk" => {
            let path = fixture.stage.join("disk.raw");
            let mut bytes = fs::read(&path).unwrap();
            bytes[0] ^= 1;
            fs::write(path, bytes).unwrap();
        }
        "changed-manifest" => {
            let path = fixture.stage.join("manifest.json");
            let text = fs::read_to_string(&path)
                .unwrap()
                .replace("\"source\"", "\"tamper\"");
            fs::write(path, text).unwrap();
        }
        "unknown-entry" => fs::write(fixture.stage.join("keep.txt"), b"unrelated bytes").unwrap(),
        "missing-member" => fs::remove_file(fixture.stage.join("vars.fd")).unwrap(),
        "record-symlink" => {
            fs::rename(&record, &retained).unwrap();
            symlink(&retained, record).unwrap();
        }
        "pin-symlink" => {
            fs::rename(&pin, &retained).unwrap();
            symlink(&record, pin).unwrap();
        }
        "disk-symlink" => {
            let disk = fixture.stage.join("disk.raw");
            fs::rename(&disk, &retained).unwrap();
            symlink(&retained, disk).unwrap();
        }
        "record-fifo" => fifo(&record),
        "pin-fifo" => fifo(&pin),
        "disk-fifo" => fifo(&fixture.stage.join("disk.raw")),
        _ => panic!("unknown ownership mutation {scenario}"),
    }
}
