use super::*;

fn assert_owned(path: &Path) {
    assert_eq!(
        MediaLease::acquire([path]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

fn alias(s: &Scratch, path: &Path, name: &str) -> PathBuf {
    let alias = s.path(name);
    fs::hard_link(path, &alias).unwrap();
    alias
}

#[test]
fn repeated_restore_owns_new_inodes_and_releases_old_generations() {
    let (s, disk, vars, snapshot) = fixture("managed-repeat-ownership");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    let mut previous: Option<(PathBuf, PathBuf)> = None;
    for generation in 0..3 {
        pair.restore(&snapshot).unwrap();
        let (selected_disk, selected_vars) = pair.paths().unwrap();
        let disk_alias = alias(&s, &selected_disk, &format!("disk-alias-{generation}"));
        let vars_alias = alias(&s, &selected_vars, &format!("vars-alias-{generation}"));
        for path in [&selected_disk, &selected_vars, &disk_alias, &vars_alias] {
            assert_owned(path);
        }
        assert_owned(&disk);
        assert_owned(&vars);
        if let Some((old_disk, old_vars)) = previous.take() {
            MediaLease::acquire([old_disk.as_path(), old_vars.as_path()]).unwrap();
        }
        previous = Some((disk_alias, vars_alias));
    }
    let selected = pair.paths().unwrap();
    drop(pair);
    let (disk_alias, vars_alias) = previous.unwrap();
    MediaLease::acquire([
        selected.0.as_path(),
        selected.1.as_path(),
        disk_alias.as_path(),
        vars_alias.as_path(),
    ])
    .unwrap();
}

#[test]
fn publication_keeps_staged_and_selected_aliases_owned() {
    let (s, disk, vars, snapshot) = fixture("managed-publish-ownership");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    for generation in 0..2 {
        let old = pair.paths().unwrap();
        pair.restore_using(&snapshot, |staged, current| {
            let staged_alias = alias(
                &s,
                &staged.join("disk.raw"),
                &format!("publishing-alias-{generation}"),
            );
            assert_owned(&old.0);
            assert_owned(&old.1);
            assert_owned(&staged.join("disk.raw"));
            assert_owned(&staged.join("vars.fd"));
            assert_owned(&staged_alias);
            crate::snapshot_pair::snapshot_publish::publish(staged, current)?;
            assert_owned(&current.join("disk.raw"));
            assert_owned(&current.join("vars.fd"));
            assert_owned(&staged_alias);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn publication_errors_retain_old_and_new_inode_ownership() {
    for already_managed in [false, true] {
        for after_publish in [false, true] {
            let (s, disk, vars, snapshot) = fixture("managed-error-ownership");
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            if already_managed {
                pair.restore(&snapshot).unwrap();
            }
            let old = pair.paths().unwrap();
            let old_alias = alias(&s, &old.0, "old-alias");
            let staged_alias = s.path("staged-alias");
            let result = pair.restore_using(&snapshot, |staged, current| {
                fs::hard_link(staged.join("disk.raw"), &staged_alias)?;
                if after_publish {
                    crate::snapshot_pair::snapshot_publish::publish(staged, current)?;
                }
                Err(io::Error::other("injected publication uncertainty"))
            });
            assert!(result.is_err());
            let selected = pair.paths().unwrap();
            for path in [&selected.0, &selected.1, &old_alias, &staged_alias] {
                assert_owned(path);
            }
            pair.restore(&snapshot).unwrap();
            MediaLease::acquire([staged_alias.as_path()]).unwrap();
            if already_managed {
                MediaLease::acquire([old_alias.as_path()]).unwrap();
            } else {
                assert_owned(&old_alias);
            }
            let recovered = pair.paths().unwrap();
            assert_owned(&recovered.0);
            assert_owned(&recovered.1);
            drop(pair);
            MediaLease::acquire([old_alias.as_path(), staged_alias.as_path()]).unwrap();
        }
    }
}
