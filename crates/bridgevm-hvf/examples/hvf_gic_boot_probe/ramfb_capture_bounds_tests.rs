use super::*;
use bridgevm_hvf::ramfb::DRM_FORMAT_XRGB8888;
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

struct Memory {
    calls: Cell<usize>,
    bytes: Vec<u8>,
}
impl GuestMemoryMut for Memory {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        false
    }
    fn read_bytes(&self, _: u64, len: usize) -> Option<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        self.bytes.get(..len).map(|bytes| bytes.to_vec())
    }
}

struct OwnedDirectory(PathBuf);
impl OwnedDirectory {
    fn new() -> Self {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bridgevm-capture-limit-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn policy(&self, raw_limit: u64, ppm_limit: u64) -> Policy {
        Policy {
            raw_limit,
            ppm_limit,
            directory: self.0.clone(),
        }
    }
}
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn config() -> RamfbConfig {
    RamfbConfig {
        addr: 1,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 2,
        height: 2,
        stride: 8,
    }
}
fn memory() -> Memory {
    Memory {
        calls: Cell::new(0),
        bytes: vec![0x42; 16],
    }
}

#[test]
fn oversized_frame_refuses_before_guest_read_or_any_artifact() {
    let directory = OwnedDirectory::new();
    let memory = memory();
    let mut config = config();
    config.width = 2048;
    config.height = 8193;
    config.stride = 8192;
    let result = read_with_policy(
        &memory,
        config,
        Some(&directory.policy(RAW_LIMIT, PPM_LIMIT)),
    );
    assert!(matches!(
        result,
        Err(CaptureError::ByteLimit {
            raw_bytes: 67_117_056,
            ..
        })
    ));
    assert_eq!(memory.calls.get(), 0);
    let files: Vec<_> = fs::read_dir(&directory.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"capture-bound-refused.json".into()));
    assert!(files.contains(&"capture-bound-status.json".into()));
    assert!(
        fs::read_to_string(directory.0.join("capture-bound-status.json"))
            .unwrap()
            .contains("\"refused\":true")
    );
}

#[test]
fn exact_raw_and_ppm_header_limits_keep_all_valid_bytes() {
    let directory = OwnedDirectory::new();
    let memory = memory();
    let (_, ppm_len) = geometry(config()).unwrap();
    let snapshot =
        read_with_policy(&memory, config(), Some(&directory.policy(16, ppm_len))).unwrap();
    assert_eq!(snapshot.bytes, memory.bytes);
    assert_eq!(snapshot.ppm_bytes().unwrap().len() as u64, ppm_len);
    assert_eq!(memory.calls.get(), 1);
    assert!(!directory.0.join("capture-bound-refused.json").exists());
}

#[test]
fn ppm_header_and_padded_stride_are_part_of_the_bound() {
    for (raw_limit, ppm_limit, stride) in [(16, 22, 8), (31, 100, 16)] {
        let directory = OwnedDirectory::new();
        let memory = memory();
        let mut config = config();
        config.stride = stride;
        assert!(matches!(
            read_with_policy(
                &memory,
                config,
                Some(&directory.policy(raw_limit, ppm_limit))
            ),
            Err(CaptureError::ByteLimit { .. })
        ));
        assert_eq!(memory.calls.get(), 0);
    }
}

#[test]
fn maximum_geometry_is_rejected_without_allocating() {
    let directory = OwnedDirectory::new();
    let memory = memory();
    let mut config = config();
    config.height = u32::MAX;
    config.width = u32::MAX;
    config.stride = u32::MAX;
    assert!(matches!(
        read_with_policy(
            &memory,
            config,
            Some(&directory.policy(RAW_LIMIT, PPM_LIMIT))
        ),
        Err(CaptureError::Snapshot(
            RamfbSnapshotError::StrideTooSmall { .. }
        ))
    ));
    assert_eq!(memory.calls.get(), 0);
}

#[test]
fn refusal_is_not_cleared_by_a_later_small_frame() {
    let directory = OwnedDirectory::new();
    let policy = directory.policy(15, 100);
    assert!(policy.validate(config()).is_err());
    directory.policy(16, 100).validate(config()).unwrap();
    assert!(
        fs::read_to_string(directory.0.join("capture-bound-status.json"))
            .unwrap()
            .contains("\"refused\":true")
    );
}

#[test]
fn status_write_failure_refuses_before_guest_read() {
    let directory = OwnedDirectory::new();
    fs::create_dir(directory.0.join("capture-bound-status.pending")).unwrap();
    let memory = memory();
    assert!(matches!(
        read_with_policy(&memory, config(), Some(&directory.policy(16, 100))),
        Err(CaptureError::Policy(_))
    ));
    assert_eq!(memory.calls.get(), 0);
}

#[test]
fn overflowing_guest_address_refuses_before_read_or_artifact() {
    let directory = OwnedDirectory::new();
    let memory = memory();
    let mut config = config();
    config.addr = u64::MAX - 5;
    assert!(matches!(
        read_with_policy(&memory, config, Some(&directory.policy(16, 100))),
        Err(CaptureError::AddressRange { .. })
    ));
    assert_eq!(memory.calls.get(), 0);
    assert!(directory.0.join("capture-bound-refused.json").is_file());
}

#[test]
fn later_status_failure_preserves_pending_refusal_evidence() {
    let directory = OwnedDirectory::new();
    let policy = directory.policy(16, 100);
    policy.validate(config()).unwrap();
    let positive = fs::read(directory.0.join("capture-bound-status.json")).unwrap();
    fs::create_dir(directory.0.join("capture-bound-status.pending")).unwrap();
    let memory = memory();
    assert!(matches!(
        read_with_policy(&memory, config(), Some(&policy)),
        Err(CaptureError::Policy(_))
    ));
    assert_eq!(memory.calls.get(), 0);
    assert_eq!(
        fs::read(directory.0.join("capture-bound-status.json")).unwrap(),
        positive
    );
    assert!(directory.0.join("capture-bound-status.pending").exists());
}

#[test]
fn no_opt_in_preserves_existing_snapshot_behaviour() {
    let memory = memory();
    let snapshot = read_with_policy(&memory, config(), None).unwrap();
    assert_eq!(snapshot.bytes, memory.bytes);
    assert_eq!(memory.calls.get(), 1);
}
