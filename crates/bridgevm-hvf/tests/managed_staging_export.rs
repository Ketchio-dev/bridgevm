//! A completed export is not another VM's abandoned restore staging.

use bridgevm_hvf::snapshot_pair::{create_snapshot, restore_snapshot, verify_snapshot};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CHILD: &str = "BRIDGEVM_TEST_MANAGED_EXPORT_CHILD";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let base = std::env::temp_dir();
        for sequence in 0..1000 {
            let path = base.join(format!("managed-export-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create owned fixture: {error}"),
            }
        }
        panic!("fixture namespace exhausted");
    }

    fn pair(&self, name: &str) -> (PathBuf, PathBuf) {
        let root = self.0.join(name);
        fs::create_dir(&root).unwrap();
        let disk = root.join("disk");
        let vars = root.join("vars");
        fs::write(&disk, format!("{name} disk")).unwrap();
        fs::write(&vars, format!("{name} vars")).unwrap();
        (disk, vars)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn contents(directory: &Path) -> Vec<Vec<u8>> {
    ["disk.raw", "vars.fd", "manifest.json"]
        .map(|name| fs::read(directory.join(name)).unwrap())
        .into()
}

#[test]
fn completed_export_survives_managed_restore() {
    if std::env::var_os(CHILD).is_none() {
        let result = Command::new("/bin/sh")
            .args(["-c", "umask 077\nexec \"$@\"", "managed-export-test"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "completed_export_survives_managed_restore",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    // The subprocess inherits a private user umask, as real CLI helpers do,
    // without changing the parent test process or chmod of the snapshot.
    let scratch = Scratch::new();
    let (a_disk, a_vars) = scratch.pair("a");
    let (b_disk, b_vars) = scratch.pair("b");
    let snapshot = scratch.0.join("a.snapshot");
    create_snapshot(&a_disk, &a_vars, &snapshot, "a", false, 1024).unwrap();
    restore_snapshot(&snapshot, &a_disk, &a_vars, false).unwrap();
    let managed = fs::read_dir(a_disk.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".bridgevm-pair-v2-")
        })
        .unwrap();
    let selected = contents(&managed.join("current"));
    let exported = managed.join("staging");
    create_snapshot(&b_disk, &b_vars, &exported, "b", false, 1024).unwrap();
    let exported_manifest = verify_snapshot(&exported).unwrap();
    let exported_contents = contents(&exported);
    assert_eq!(
        fs::metadata(&exported).unwrap().permissions().mode() & 0o777,
        0o700
    );

    let result = restore_snapshot(&snapshot, &a_disk, &a_vars, false);

    assert!(
        exported.exists(),
        "completed export was deleted; restore result: {result:?}"
    );
    assert_eq!(contents(&exported), exported_contents);
    assert_eq!(verify_snapshot(&exported).unwrap(), exported_manifest);
    assert!(
        result.is_err(),
        "ambiguous completed staging must require explicit resolution"
    );
    assert_eq!(contents(&managed.join("current")), selected);
    assert_eq!(fs::read(&a_disk).unwrap(), b"a disk");
    assert_eq!(fs::read(&a_vars).unwrap(), b"a vars");
    assert_eq!(fs::read(&b_disk).unwrap(), b"b disk");
    assert_eq!(fs::read(&b_vars).unwrap(), b"b vars");
}
