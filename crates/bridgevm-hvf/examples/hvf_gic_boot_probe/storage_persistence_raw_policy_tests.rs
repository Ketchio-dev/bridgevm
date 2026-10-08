use super::*;

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
                persist_nvme_media(&mut platform, disk, namespace, &mut owner),
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

        assert!(persist_nvme_media(&mut platform, disk, namespace, &mut owner).is_empty());
        assert_eq!(fs::read(&output).unwrap(), [0xd1; 512]);
        assert_eq!(fs::read(&disk.path).unwrap(), [0xa1; 512]);
        assert_owned(&disk.path);
        assert_eq!(fs::read(&media.flash_vars.path).unwrap(), [0xa2; 512]);
        drop(MediaLease::acquire([output.as_path()]).unwrap());
        drop(owner);
    }
}
