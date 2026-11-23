use super::*;
use bridgevm_hvf::media_lease::MediaLease;
use bridgevm_hvf::snapshot_pair::managed::runtime::acquire;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};

/// A uniquely owned scratch directory for native persistence fixtures.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("bridgevm-owned-stop-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create fixture: {error}"),
            }
        }
    }

    fn media(&self, name: &str) -> WritableMedia {
        let path = self.0.join(name);
        fs::write(&path, b"initial").unwrap();
        WritableMedia::new(path).with_write_back(true)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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

    let result = persist_nvme_media(
            &mut platform,
            media,
            NvmePersistNamespace::Primary,
            &mut owner_a,
        );
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

fn raw_media(s: &Scratch, namespace: NvmePersistNamespace) -> VirtBootMediaConfig {
    let mut media = runtime_media(s, "input", 0xa1, 0xa2);
    if matches!(namespace, NvmePersistNamespace::Target) {
        media.nvme_target = media.nvme_disk.take();
    }
    media
}

fn disk_mut(media: &mut VirtBootMediaConfig, namespace: NvmePersistNamespace) -> &mut WritableMedia {
    match namespace {
        NvmePersistNamespace::Primary => media.nvme_disk.as_mut().unwrap(),
        NvmePersistNamespace::Target => media.nvme_target.as_mut().unwrap(),
    }
}

fn attach_raw(media: &WritableMedia, namespace: NvmePersistNamespace) -> VirtPlatform {
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    match namespace {
        NvmePersistNamespace::Primary => platform.attach_nvme_raw_file(&media.path, media.write_back),
        NvmePersistNamespace::Target => {
            platform.attach_nvme_second_namespace_raw_file(&media.path, media.write_back)
        }
    }
    .unwrap();
    assert!(namespace.image_if_memory(&platform).is_none());
    assert_eq!(namespace.disk_len(&platform), 512);
    platform
}

fn assert_owned(path: &Path) {
    assert_eq!(
        MediaLease::acquire([path]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock,
        "{} must remain leased", path.display()
    );
}

fn snapshot_write(path: &Path) -> MediaWrite {
    MediaWrite { kind: MediaWriteKind::Snapshot, path: path.to_path_buf(), bytes: 512 }
}

#[test]
fn raw_snapshot_success_retains_primary_and_nsid2_inputs_and_outputs() {
    let s = Scratch::new();
    let mut media = runtime_media(&s, "primary", 0xa1, 0xa2);
    media.nvme_target = runtime_media(&s, "target", 0xb1, 0xb2).nvme_disk;
    let primary_out = s.0.join("primary-out.raw");
    let target_out = s.0.join("target-out.raw");
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(primary_out.clone());
    media.nvme_target.as_mut().unwrap().snapshot_path = Some(target_out.clone());
    let mut owner = acquire(&mut media).unwrap();
    let primary = media.nvme_disk.as_ref().unwrap();
    let target = media.nvme_target.as_ref().unwrap();
    let mut platform = attach_raw(primary, NvmePersistNamespace::Primary);
    platform.attach_nvme_second_namespace_raw_file(&target.path, false).unwrap();
    assert!(platform.nvme_second_namespace_disk_if_memory().is_none());
    assert_eq!(platform.nvme_second_namespace_disk_len(), Some(512));
    let mut paths = vec![primary.path.clone(), target.path.clone(), media.flash_vars.path.clone()];
    for (namespace, disk, output, byte) in [
        (NvmePersistNamespace::Primary, primary, &primary_out, 0xa1),
        (NvmePersistNamespace::Target, target, &target_out, 0xb1),
    ] {
        assert_eq!(
            persist_nvme_media(&mut platform, disk, namespace, &mut owner).unwrap(),
            vec![snapshot_write(output)]
        );
        assert_eq!(fs::read(output).unwrap(), [byte; 512]);
        assert_eq!(fs::read(&disk.path).unwrap(), [byte; 512]);
        let alias = output.with_extension("alias");
        fs::hard_link(output, &alias).unwrap();
        paths.extend([output.clone(), alias]);
        for path in &paths {
            assert_owned(path);
        }
    }
    assert_eq!(fs::read(&media.flash_vars.path).unwrap(), [0xa2; 512]);
    drop(owner);
    for path in paths {
        drop(MediaLease::acquire([path.as_path()]).unwrap());
    }
}

#[test]
fn raw_primary_and_nsid2_contention_refuses_then_retries_after_owner_drop() {
    for namespace in [NvmePersistNamespace::Primary, NvmePersistNamespace::Target] {
        let s = Scratch::new();
        let mut b = runtime_media(&s, "syntheticB", 0xb1, 0xb2);
        let owner_b = acquire(&mut b).unwrap();
        let disk_b = &b.nvme_disk.as_ref().unwrap().path;
        let mut media = raw_media(&s, namespace);
        disk_mut(&mut media, namespace).snapshot_path = Some(disk_b.clone());
        let mut owner_a = acquire(&mut media).unwrap();
        let disk = disk_mut(&mut media, namespace);
        let mut platform = attach_raw(disk, namespace);
        assert_owned(disk_b);
        let result = persist_nvme_media(&mut platform, disk, namespace, &mut owner_a);
        let disk_unchanged = fs::read(disk_b).unwrap() == [0xb1; 512];
        let vars_unchanged = fs::read(&b.flash_vars.path).unwrap() == [0xb2; 512];
        assert!(
            result.is_err() && disk_unchanged && vars_unchanged,
            "{}: result={result:?}, disk_unchanged={disk_unchanged}, vars_unchanged={vars_unchanged}",
            namespace.subject()
        );
        assert_owned(disk_b);
        assert_owned(&b.flash_vars.path);
        assert_owned(&disk.path);
        drop(owner_b);
        assert_eq!(
            persist_nvme_media(&mut platform, disk, namespace, &mut owner_a).unwrap(),
            vec![snapshot_write(disk_b)]
        );
        assert_eq!(fs::read(disk_b).unwrap(), [0xa1; 512]);
        assert_eq!(fs::read(&b.flash_vars.path).unwrap(), [0xb2; 512]);
        assert_owned(disk_b);
        drop(owner_a);
        drop(MediaLease::acquire([disk_b.as_path()]).unwrap());
    }
}

#[test]
fn raw_snapshot_uses_captured_some_after_retarget_or_removal() {
    for namespace in [NvmePersistNamespace::Primary, NvmePersistNamespace::Target] {
        for retarget in [true, false] {
            let s = Scratch::new();
            let captured = s.0.join("captured.raw");
            let other = s.0.join("other.raw");
            fs::write(&captured, [0xc1; 512]).unwrap();
            fs::write(&other, [0xd1; 512]).unwrap();
            let mut media = raw_media(&s, namespace);
            disk_mut(&mut media, namespace).snapshot_path = Some(captured.clone());
            let mut owner = acquire(&mut media).unwrap();
            let disk = disk_mut(&mut media, namespace);
            let mut platform = attach_raw(disk, namespace);
            disk.snapshot_path = retarget.then(|| other.clone());

            assert_eq!(
                persist_nvme_media(&mut platform, disk, namespace, &mut owner).unwrap(),
                vec![snapshot_write(&captured)]
            );
            assert_eq!(fs::read(&captured).unwrap(), [0xa1; 512]);
            assert_eq!(fs::read(&other).unwrap(), [0xd1; 512]);
            assert_eq!(fs::read(&disk.path).unwrap(), [0xa1; 512]);
            let alias = s.0.join("captured-alias");
            fs::hard_link(&captured, &alias).unwrap();
            assert_owned(&captured);
            assert_owned(&alias);
            drop(MediaLease::acquire([other.as_path()]).unwrap());
            assert_eq!(fs::read(&media.flash_vars.path).unwrap(), [0xa2; 512]);
            drop(owner);
            drop(MediaLease::acquire([captured.as_path(), alias.as_path()]).unwrap());
        }
    }
}

#[test]
fn raw_snapshot_captured_none_ignores_newly_configured_destination() {
    for namespace in [NvmePersistNamespace::Primary, NvmePersistNamespace::Target] {
        let s = Scratch::new();
        let output = s.0.join("newly-configured.raw");
        fs::write(&output, [0xd1; 512]).unwrap();
        let mut media = raw_media(&s, namespace);
        let mut owner = acquire(&mut media).unwrap();
        let disk = disk_mut(&mut media, namespace);
        let mut platform = attach_raw(disk, namespace);
        disk.snapshot_path = Some(output.clone());

        assert!(persist_nvme_media(&mut platform, disk, namespace, &mut owner).unwrap().is_empty());
        assert_eq!(fs::read(&output).unwrap(), [0xd1; 512]);
        assert_eq!(fs::read(&disk.path).unwrap(), [0xa1; 512]);
        assert_owned(&disk.path);
        assert_eq!(fs::read(&media.flash_vars.path).unwrap(), [0xa2; 512]);
        drop(MediaLease::acquire([output.as_path()]).unwrap());
        drop(owner);
    }
}

#[test]
fn raw_snapshot_to_self_pins_source_inode_through_later_vars_persist() {
    for namespace in [NvmePersistNamespace::Primary, NvmePersistNamespace::Target] {
        let s = Scratch::new();
        let mut media = raw_media(&s, namespace);
        media.flash_vars.write_back = true;
        let vars = fs::canonicalize(&media.flash_vars.path).unwrap();
        let disk = disk_mut(&mut media, namespace);
        disk.write_back = true;
        disk.snapshot_path = Some(disk.path.clone());
        let old_source = s.0.join("old-source.raw");
        fs::hard_link(&disk.path, &old_source).unwrap();
        let original = fs::metadata(&old_source).unwrap();
        let mut owner = acquire(&mut media).unwrap();
        let disk = disk_mut(&mut media, namespace);
        let mut platform = attach_raw(disk, namespace);
        assert_owned(&old_source);

        assert_eq!(
            persist_nvme_media(&mut platform, disk, namespace, &mut owner).unwrap(),
            vec![
                snapshot_write(disk.snapshot_path.as_ref().unwrap()),
                MediaWrite { kind: MediaWriteKind::WriteBack, path: disk.path.clone(), bytes: 512 },
            ]
        );
        let published = fs::metadata(&disk.path).unwrap();
        assert_ne!((original.dev(), original.ino()), (published.dev(), published.ino()));
        let new_alias = s.0.join("new-source.raw");
        fs::hard_link(&disk.path, &new_alias).unwrap();
        assert_eq!(fs::read(&old_source).unwrap(), [0xa1; 512]);
        assert_eq!(fs::read(&disk.path).unwrap(), [0xa1; 512]);
        let old_blocked = MediaLease::acquire([old_source.as_path()])
            .err().map(|error| error.kind());
        assert_owned(&disk.path);
        assert_owned(&new_alias);
        assert_eq!(fs::read(&vars).unwrap(), [0xa2; 512]);

        assert_eq!(
            owner.persist(RuntimeMediaSlot::Vars, &[0xa3; 512]).unwrap(),
            vec![MediaWrite { kind: MediaWriteKind::WriteBack, path: vars.clone(), bytes: 512 }]
        );
        let old_blocked_after_vars = MediaLease::acquire([old_source.as_path()])
            .err().map(|error| error.kind());
        assert_eq!(
            (old_blocked, old_blocked_after_vars),
            (Some(io::ErrorKind::WouldBlock), Some(io::ErrorKind::WouldBlock)),
            "{} must pin its open raw source inode across snapshot and vars publication",
            namespace.subject()
        );
        assert_owned(&disk.path);
        assert_owned(&new_alias);
        assert_owned(&vars);
        assert_eq!(fs::read(&vars).unwrap(), [0xa3; 512]);
        assert_eq!(fs::read(&disk.path).unwrap(), [0xa1; 512]);
        assert_eq!(fs::read(&old_source).unwrap(), [0xa1; 512]);
        drop(owner);
        drop(MediaLease::acquire([
            old_source.as_path(), disk.path.as_path(), new_alias.as_path(), vars.as_path(),
        ]).unwrap());
        drop(platform);
    }
}
