use super::*;
use std::os::unix::fs::symlink;
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "bridgevm-app-resolver-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn executable(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn paired_canonical_cli_precedes_fixed_installed_locations() {
    let fixture = Fixture::new();
    let cli = fixture.executable(
        "Owned.app/Contents/Resources/target/release/bridgevm",
        b"fixture",
    );
    let alias = fixture.0.join("bridgevm-link");
    symlink(&cli, &alias).unwrap();
    let canonical = alias.canonicalize().unwrap();
    let home = fixture.0.join("owned-home");
    let paths = candidates(&canonical, Some(&home));
    assert_eq!(
        paths[0],
        fixture.0.join("Owned.app/Contents/MacOS/BridgeVMControl")
    );
    assert_eq!(
        paths[1],
        PathBuf::from("/Applications/BridgeVMControl.app/Contents/MacOS/BridgeVMControl")
    );
    assert_eq!(
        paths[2],
        home.join("Applications/BridgeVMControl.app/Contents/MacOS/BridgeVMControl")
    );
    assert_eq!(
        candidates(&fixture.0.join("target/release/bridgevm"), None).len(),
        1
    );
}

#[test]
fn missing_candidate_falls_back_but_first_old_version_refuses() {
    let fixture = Fixture::new();
    let absent = fixture.0.join("absent");
    let old = fixture.executable("old", b"old native app without a CLI");
    let compatible = fixture.executable("current", PROTOCOL_MARKER);
    assert_eq!(
        resolve_candidates(&[absent, compatible.clone()]).unwrap(),
        compatible
    );
    let error = resolve_candidates(&[old, compatible]).unwrap_err();
    assert!(format!("{error:#}").contains("this app will not be launched"));
}

#[test]
fn marker_can_cross_a_read_boundary() {
    let fixture = Fixture::new();
    let mut bytes = vec![b'x'; 64 * 1024 - 7];
    bytes.extend_from_slice(PROTOCOL_MARKER);
    bytes.extend_from_slice(b"suffix");
    let path = fixture.executable("current", &bytes);
    assert_eq!(resolve_candidates(&[path.clone()]).unwrap(), path);
}

#[test]
fn symlinks_directories_and_non_executable_files_are_refused() {
    let fixture = Fixture::new();
    let target = fixture.executable("current", PROTOCOL_MARKER);
    let link = fixture.0.join("link");
    symlink(&target, &link).unwrap();
    assert!(resolve_candidates(&[link]).is_err());
    assert!(resolve_candidates(&[fixture.0.clone()]).is_err());
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(resolve_candidates(&[target]).is_err());
}

#[test]
fn oversized_sparse_file_is_refused_before_marker_read() {
    let fixture = Fixture::new();
    let path = fixture.executable("oversized", PROTOCOL_MARKER);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(MAX_EXECUTABLE_BYTES + 1)
        .unwrap();
    let error = resolve_candidates(&[path]).unwrap_err();
    assert!(format!("{error:#}").contains("256 MiB"));
}

#[test]
fn fifo_is_refused_without_opening_it() {
    let fixture = Fixture::new();
    let path = fixture.0.join("fifo");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // The owned FIFO is never opened; discovery rejects its file type.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(resolve_candidates(&[path]).is_err());
}

#[test]
fn absent_installation_provides_installation_guidance() {
    let fixture = Fixture::new();
    let error = resolve_candidates(&[fixture.0.join("missing")])
        .unwrap_err()
        .to_string();
    assert!(error.contains("/Applications"));
    assert!(error.contains("Contents/Resources/target/release"));
}
