use super::*;
use bridgevm_hvf::media_lease::MediaLease;
use bridgevm_hvf::snapshot_pair::managed::runtime::acquire;
use std::{fs, io};
#[path = "storage_persistence_owner_tests.rs"]
mod ownership;

#[path = "storage_persistence_scratch.rs"]
mod scratch;
use scratch::Scratch;

#[test]
fn actual_stop_helper_keeps_vars_and_both_memory_namespaces_owned() {
    let s = Scratch::new();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = s.media("vars");
    media.nvme_disk = Some(s.media("primary"));
    media.nvme_target = Some(s.media("target"));
    let mut owner = acquire(&mut media).unwrap();
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    platform.load_flash_vars(b"saved-vars");
    platform.load_nvme_disk(vec![0x31; 512]);
    platform.attach_nvme_second_namespace(512);
    persist_stop_media(&mut platform, &media, &mut owner);
    let mut aliases = Vec::new();
    for slot in [&media.flash_vars, media.nvme_disk.as_ref().unwrap(), media.nvme_target.as_ref().unwrap()] {
        let alias = s.0.join(format!("alias-{}", aliases.len()));
        fs::hard_link(&slot.path, &alias).unwrap();
        assert_eq!(MediaLease::acquire([alias.as_path()]).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        aliases.push(alias);
    }
    assert_eq!(fs::read(&media.flash_vars.path).unwrap(), platform.flash_vars_image());
    assert_eq!(fs::read(&media.nvme_disk.as_ref().unwrap().path).unwrap(), vec![0x31; 512]);
    assert_eq!(fs::read(&media.nvme_target.as_ref().unwrap().path).unwrap(), vec![0; 512]);
    drop(owner);
    for alias in aliases {
        MediaLease::acquire([alias.as_path()]).unwrap();
    }
}
