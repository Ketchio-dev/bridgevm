use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
#[path = "storage_persistence_raw_tests.rs"]
mod raw;

fn runtime_media(s: &Scratch, name: &str, disk: u8, vars: u8) -> VirtBootMediaConfig {
    let disk_path = s.0.join(format!("{name}.raw"));
    let vars_path = s.0.join(format!("{name}.vars"));
    fs::write(&disk_path, [disk; 512]).unwrap();
    fs::write(&vars_path, [vars; 512]).unwrap();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.nvme_disk = Some(WritableMedia::new(disk_path));
    media.flash_vars = WritableMedia::new(vars_path);
    media
}

#[test]
fn raw_snapshot_refuses_another_runtime_owned_destination() {
    let s = Scratch::new();
    let mut b = runtime_media(&s, "syntheticB", 0xb1, 0xb2);
    let owner_b = acquire(&mut b).unwrap();
    let disk_b = &b.nvme_disk.as_ref().unwrap().path;
    let mut a = runtime_media(&s, "input", 0xa1, 0xa2);
    a.nvme_disk.as_mut().unwrap().snapshot_path = Some(disk_b.clone());
    let mut owner_a = acquire(&mut a).unwrap();
    let media = a.nvme_disk.as_ref().unwrap();
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    platform
        .attach_nvme_raw_file(&media.path, media.write_back)
        .unwrap();
    assert!(platform.nvme_disk_if_memory().is_none());
    assert_eq!(platform.nvme_disk_len(), 512);

    let result = catch_unwind(AssertUnwindSafe(|| {
        persist_nvme_media(
            &mut platform,
            media,
            NvmePersistNamespace::Primary,
            &mut owner_a,
        )
    }));
    let disk_unchanged = fs::read(disk_b).unwrap() == [0xb1; 512];
    let vars_unchanged = fs::read(&b.flash_vars.path).unwrap() == [0xb2; 512];
    assert!(
        result.is_err() && disk_unchanged && vars_unchanged,
        "raw snapshot must refuse runtime B's leased destination without changing disk/vars; \
         result={result:?}, disk_unchanged={disk_unchanged}, vars_unchanged={vars_unchanged}"
    );
    drop((owner_a, owner_b));
}

#[test]
fn memory_persist_refuses_another_runtime_owned_destination() {
    let s = Scratch::new();
    let mut b = runtime_media(&s, "syntheticB", 0xb1, 0xb2);
    let owner_b = acquire(&mut b).unwrap();
    let disk_b = &b.nvme_disk.as_ref().unwrap().path;
    let mut a = runtime_media(&s, "input", 0xa1, 0xa2);
    a.nvme_disk.as_mut().unwrap().snapshot_path = Some(disk_b.clone());
    let mut owner_a = acquire(&mut a).unwrap();
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    platform.load_nvme_disk(vec![0xa1; 512]);

    let error = owner_a
        .persist(
            RuntimeMediaSlot::Primary,
            platform.nvme_disk_if_memory().unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(fs::read(disk_b).unwrap(), [0xb1; 512]);
    assert_eq!(fs::read(&b.flash_vars.path).unwrap(), [0xb2; 512]);
    drop((owner_a, owner_b));
}
