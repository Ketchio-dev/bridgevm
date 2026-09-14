use super::*;

#[test]
fn atomic_publication_owns_both_inodes_and_recovers_from_uncertain_errors() {
    for managed in [false, true] {
        for after_rename in [false, true] {
            let (s, mut media, snapshot) = fixture("runtime-persist-atomic-errors");
            if managed {
                LockedPair::open(
                    &media.nvme_disk.as_ref().unwrap().path,
                    &media.flash_vars.path,
                )
                .unwrap()
                .restore(&snapshot)
                .unwrap();
            }
            let mut guard = acquire(&mut media).unwrap();
            let destination = media.flash_vars.path.clone();
            let old_bytes = fs::read(&destination).unwrap();
            let old_alias = s.path("old-alias");
            let new_alias = s.path("new-alias");
            fs::hard_link(&destination, &old_alias).unwrap();
            let result =
                guard.persist_using(RuntimeMediaSlot::Vars, b"next-vars", |staged, dest| {
                    fs::hard_link(staged, &new_alias)?;
                    for path in [staged, dest, old_alias.as_path(), new_alias.as_path()] {
                        assert_owned(path);
                    }
                    if after_rename {
                        fs::rename(staged, dest)?;
                        assert_owned(dest);
                        assert_owned(&new_alias);
                    }
                    Err(io::Error::other("injected publication error"))
                });
            assert!(result.is_err());
            assert_owned(&old_alias);
            assert_owned(&new_alias);
            assert_eq!(
                fs::read(&destination).unwrap(),
                if after_rename {
                    b"next-vars".to_vec()
                } else {
                    old_bytes
                }
            );
            guard
                .persist(RuntimeMediaSlot::Primary, b"next-disk")
                .unwrap();
            assert_owned(&destination);
            if after_rename {
                assert_owned(&new_alias);
            }
            guard
                .persist(RuntimeMediaSlot::Vars, b"recovered-vars")
                .unwrap();
            MediaLease::acquire([old_alias.as_path(), new_alias.as_path()]).unwrap();
            assert_owned(&destination);
            drop(guard);
            MediaLease::acquire([destination.as_path()]).unwrap();
        }
    }
}

#[test]
fn owned_staging_never_truncates_preexisting_temp_aliases() {
    let (s, mut media, _) = fixture("runtime-persist-temp-alias");
    let original = s.write("unrelated", b"preserve");
    fs::hard_link(&original, s.path(".vars.tmp")).unwrap();
    let mut guard = acquire(&mut media).unwrap();
    guard
        .persist(RuntimeMediaSlot::Vars, b"saved-vars")
        .unwrap();
    assert_eq!(fs::read(original).unwrap(), b"preserve");
    assert_eq!(fs::read(s.path(".vars.tmp")).unwrap(), b"preserve");
}

#[test]
fn staging_cleanup_does_not_remove_a_replacement_at_the_old_name() {
    let (_s, mut media, _) = fixture("runtime-persist-temp-replacement");
    let mut guard = acquire(&mut media).unwrap();
    let mut replacement = None;
    guard
        .persist_using(RuntimeMediaSlot::Vars, b"saved", |staged, destination| {
            fs::rename(staged, destination)?;
            fs::write(staged, b"replacement")?;
            replacement = Some(staged.to_path_buf());
            Ok(())
        })
        .unwrap();
    assert_eq!(fs::read(replacement.unwrap()).unwrap(), b"replacement");
}
