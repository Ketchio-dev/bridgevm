use super::*;
use std::os::unix::fs::MetadataExt;

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
            persist_nvme_media(&mut platform, disk, namespace, &mut owner),
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
