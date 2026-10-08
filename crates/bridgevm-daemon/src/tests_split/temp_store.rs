//! Test-scoped ownership, separate from persistent production VmStore.
use bridgevm_storage::VmStore;
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct TestStoreRoot {
    path: PathBuf,
    identity: (u64, u64),
}

pub(super) fn temp_store() -> (TestStoreRoot, VmStore) {
    let parent = std::env::var_os("BRIDGEVM_DAEMON_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let owned = allocate(&parent).expect("allocate owned test store");
    require_socket_root(owned.1.root()).expect(
        "daemon fixture needs a shorter canonical BRIDGEVM_DAEMON_TEST_ROOT for Unix sockets",
    );
    owned
}

fn require_socket_root(root: &Path) -> io::Result<()> {
    let bytes = root.as_os_str().as_encoded_bytes().len()
        + b"/vms/legacy.vmbridge/metadata/guest-tools.sock".len();
    let capacity = if cfg!(target_os = "macos") { 104 } else { 108 };
    if bytes >= capacity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "nested Unix socket pathname is {bytes} bytes; maximum is {}",
                capacity - 1
            ),
        ));
    }
    Ok(())
}

fn allocate(parent: &Path) -> io::Result<(TestStoreRoot, VmStore)> {
    allocate_with(parent, || NEXT.fetch_add(1, Ordering::Relaxed))
}

fn allocate_with(
    parent: &Path,
    mut next: impl FnMut() -> u64,
) -> io::Result<(TestStoreRoot, VmStore)> {
    let parent = parent.canonicalize()?;
    for _ in 0..128 {
        let id = next();
        let path = parent.join(format!("bvmd-{}-{id}", std::process::id()));
        match DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {
                let info = fs::symlink_metadata(&path)?;
                let store = VmStore::new(path.clone());
                return Ok((
                    TestStoreRoot {
                        path,
                        identity: (info.dev(), info.ino()),
                    },
                    store,
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "test store candidates exhausted",
    ))
}

impl Drop for TestStoreRoot {
    fn drop(&mut self) {
        let outcome = match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Ok(info) if info.is_dir() && (info.dev(), info.ino()) == self.identity => {
                fs::remove_dir_all(&self.path)
            }
            _ => Err(io::Error::other(
                "test store root replaced; refusing removal",
            )),
        };
        if let Err(error) = outcome {
            if std::thread::panicking() {
                eprintln!("owned test store cleanup failed: {error}");
            } else {
                panic!("owned test store cleanup failed: {error}");
            }
        }
    }
}

#[test]
fn socket_root_admission_counts_bytes_and_refused_scope_is_cleaned() {
    let capacity = if cfg!(target_os = "macos") { 104 } else { 108 };
    let tail = b"/vms/legacy.vmbridge/metadata/guest-tools.sock".len();
    assert!(require_socket_root(Path::new(&"a".repeat(capacity - tail - 1))).is_ok());
    assert!(require_socket_root(Path::new(&"a".repeat(capacity - tail))).is_err());
    let (_parent, store) = temp_store();
    let nested = store.root().join("long-".repeat(20));
    fs::create_dir(&nested).unwrap();
    assert!(std::panic::catch_unwind(|| {
        let (_root, allocated) = allocate(&nested).unwrap();
        require_socket_root(allocated.root()).unwrap();
    })
    .is_err());
    assert_eq!(fs::read_dir(nested).unwrap().count(), 0);
}

#[test]
fn allocation_collision_never_adopts_an_existing_tree() {
    let (_parent, store) = temp_store();
    let occupied = store.root().join(format!("bvmd-{}-7", std::process::id()));
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("keep"), b"foreign").unwrap();
    let mut candidates = [7, 8].into_iter();
    let (owned, allocated) = allocate_with(store.root(), || candidates.next().unwrap()).unwrap();
    assert_ne!(allocated.root(), occupied);
    drop(owned);
    assert!(!allocated.root().exists());
    assert_eq!(fs::read(occupied.join("keep")).unwrap(), b"foreign");
}

#[test]
fn replaced_root_is_not_removed() {
    let (_parent, store) = temp_store();
    let (owned, allocated) = allocate(store.root()).unwrap();
    let renamed = allocated.root().with_extension("saved");
    fs::rename(allocated.root(), &renamed).unwrap();
    fs::create_dir(allocated.root()).unwrap();
    fs::write(allocated.root().join("keep"), b"replacement").unwrap();
    assert!(std::panic::catch_unwind(|| drop(owned)).is_err());
    assert_eq!(
        fs::read(allocated.root().join("keep")).unwrap(),
        b"replacement"
    );
    assert!(renamed.is_dir());
}

#[test]
fn scoped_store_cleanup_survives_unwind_and_preserves_another_fixture() {
    let (_other_root, other) = temp_store();
    fs::write(other.root().join("keep"), b"other").unwrap();
    let normal_path;
    {
        let (_owner, store) = temp_store();
        normal_path = store.root().to_owned();
        fs::write(store.root().join("artifact"), b"normal").unwrap();
    }
    assert!(!normal_path.exists());
    let mut failed_path = PathBuf::new();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (_owner, store) = temp_store();
        failed_path = store.root().to_owned();
        fs::write(store.root().join("artifact"), b"panic").unwrap();
        panic!("fixture assertion failure");
    }))
    .is_err());
    assert!(!failed_path.exists());
    assert_eq!(fs::read(other.root().join("keep")).unwrap(), b"other");
}
