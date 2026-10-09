use super::*;
#[path = "storage_persistence_raw_policy_tests.rs"]
mod policy;
#[path = "storage_persistence_raw_source_tests.rs"]
mod source;

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
            persist_nvme_media(&mut platform, disk, namespace, &mut owner),
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
        let result = catch_unwind(AssertUnwindSafe(|| {
            persist_nvme_media(&mut platform, disk, namespace, &mut owner_a)
        }));
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
            persist_nvme_media(&mut platform, disk, namespace, &mut owner_a),
            vec![snapshot_write(disk_b)]
        );
        assert_eq!(fs::read(disk_b).unwrap(), [0xa1; 512]);
        assert_eq!(fs::read(&b.flash_vars.path).unwrap(), [0xb2; 512]);
        assert_owned(disk_b);
        drop(owner_a);
        drop(MediaLease::acquire([disk_b.as_path()]).unwrap());
    }
}
