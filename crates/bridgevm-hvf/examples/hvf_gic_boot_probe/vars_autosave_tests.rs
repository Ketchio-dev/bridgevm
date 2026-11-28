use super::*;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::media::{read_file_into, VirtBootMediaConfig, WritableMedia};
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};
use bridgevm_hvf::snapshot_pair::managed::runtime::acquire;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bridgevm-vars-autosave-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("metadata")).unwrap();
        Self(path)
    }

    /// A written-back disk and vars pair, owned like a run's media, with the
    /// guest's flash bank read from the selected vars file.
    fn running_vm(&self) -> (PathBuf, Arc<Mutex<VirtPlatform>>, Mutex<RuntimeLease>) {
        let media = |name: &str, bytes: &[u8]| {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            WritableMedia::new(path).with_write_back(true)
        };
        let mut config = VirtBootMediaConfig::qemu_defaults();
        config.nvme_disk = Some(media("disk.raw", &[0; 512]));
        config.flash_vars = media("metadata/vars.fd", &[0xff; 0x1000]);
        let owner = acquire(&mut config).unwrap();
        let mut platform = VirtPlatform::new(VirtFdtConfig::default());
        read_file_into(&config.flash_vars.path, platform.flash_vars_mut()).unwrap();
        let platform = Arc::new(Mutex::new(platform));
        (config.flash_vars.path, platform, Mutex::new(owner))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::set_permissions(self.0.join("metadata"), fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The NOR commands need no guest RAM.
struct NoRam;

impl GuestMemoryMut for NoRam {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
}

/// The guest's 32-bit NOR flash writes, as the vCPUs dispatch them.
fn guest_writes(platform: &Mutex<VirtPlatform>, writes: &[(u64, u64)]) {
    let mut platform = platform.lock().unwrap();
    for &(offset, value) in writes {
        let op = MmioOp::Write { size: 4, value };
        let outcome = platform.on_mmio(machine::FLASH_VARS.base + offset, op, &mut NoRam);
        assert_eq!(outcome, MmioOutcome::WriteAck);
    }
}

fn program_word(platform: &Mutex<VirtPlatform>, offset: u64, value: u64) {
    guest_writes(
        platform,
        &[(offset, 0x0040_0040), (offset, value), (0, 0x00ff_00ff)],
    );
}

fn erase_block(platform: &Mutex<VirtPlatform>, offset: u64) {
    guest_writes(
        platform,
        &[
            (offset, 0x0020_0020),
            (offset, 0x00d0_00d0),
            (0, 0x00ff_00ff),
        ],
    );
}

/// An atomic replacement gives the vars file a new inode.
fn inode(path: &Path) -> u64 {
    fs::metadata(path).unwrap().ino()
}

#[test]
fn a_guest_vars_change_reaches_the_vars_file_a_quiet_second_later_without_a_stop() {
    let scratch = Scratch::new();
    let (vars, platform, owner) = scratch.running_vm();
    let mut autosave = VarsAutosave::default();
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    let original = inode(&vars);

    // Nothing changed: no write, however long the VM runs.
    assert!(!autosave.tick(&platform, &owner, at(5_000)));
    // A word program: saved once it has been quiet for a second.
    program_word(&platform, 0x100, 0x1234_5678);
    assert!(!autosave.tick(&platform, &owner, at(5_000)));
    assert!(!autosave.tick(&platform, &owner, at(5_900)));
    assert_eq!(inode(&vars), original);
    assert!(autosave.tick(&platform, &owner, at(6_000)));
    let saved = fs::read(&vars).unwrap();
    assert_eq!(saved.len(), machine::FLASH_VARS.size as usize);
    assert_eq!(saved[0x100..0x104], 0x1234_5678u32.to_le_bytes());
    // Unchanged since: never written again.
    let programmed = inode(&vars);
    assert!(!autosave.tick(&platform, &owner, at(60_000)));
    assert_eq!(inode(&vars), programmed);
    // A block erase is a change too.
    erase_block(&platform, 0);
    assert!(!autosave.tick(&platform, &owner, at(60_000)));
    assert!(autosave.tick(&platform, &owner, at(61_000)));
    assert_eq!(fs::read(&vars).unwrap()[0x100..0x104], [0xff; 4]);
}

#[test]
fn a_failed_save_keeps_running_and_is_tried_again() {
    let scratch = Scratch::new();
    let (vars, platform, owner) = scratch.running_vm();
    let mut autosave = VarsAutosave::default();
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    program_word(&platform, 0x100, 0x1234_5678);
    // The replacement's staged file cannot be created beside the vars.
    let folder = vars.parent().unwrap();
    fs::set_permissions(folder, fs::Permissions::from_mode(0o555)).unwrap();
    assert!(!autosave.tick(&platform, &owner, at(0)));
    assert!(!autosave.tick(&platform, &owner, at(1_000)));
    assert!(autosave.failure_reported);

    fs::set_permissions(folder, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!autosave.tick(&platform, &owner, at(10_999)));
    assert!(autosave.tick(&platform, &owner, at(11_000)));
    assert_eq!(
        fs::read(&vars).unwrap()[0x100..0x104],
        0x1234_5678u32.to_le_bytes()
    );
}
