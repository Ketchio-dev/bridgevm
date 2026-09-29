use super::*;

#[test]
fn unrelated_staging_contents_are_refused_and_left_intact() {
    let s = Scratch::new("admission-staging-unrelated");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    let staging = staging_path(&dest);
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("keep.txt"), b"unclaimed staging data").unwrap();
    let before = tree(&staging);
    let result = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA);
    assert_eq!(tree(&staging), before, "export deleted unclaimed staging");
    assert_refused(result, "unrelated staging");
    assert!(!dest.exists(), "refusal published a snapshot");
}

#[test]
fn staging_that_is_not_bridgevm_debris_is_refused_and_left_intact() {
    for case in ["symlink", "file", "nested-directory", "linked-member"] {
        let s = Scratch::new(&format!("admission-staging-{case}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        let outside = s.path("outside");
        fs::create_dir(&outside).unwrap();
        s.write("outside/keep.txt", b"outside data");
        match case {
            "symlink" => symlink(&outside, &staging).unwrap(),
            "file" => fs::write(&staging, b"a user file").unwrap(),
            "nested-directory" => {
                fs::create_dir_all(staging.join(DISK_NAME)).unwrap();
                fs::write(staging.join(DISK_NAME).join("keep.txt"), b"nested").unwrap();
            }
            "linked-member" => {
                fs::create_dir(&staging).unwrap();
                symlink(outside.join("keep.txt"), staging.join(VARS_NAME)).unwrap();
            }
            _ => unreachable!(),
        }
        let before = (tree(&staging), tree(&outside));
        let result = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA);
        assert_eq!((tree(&staging), tree(&outside)), before, "{case}: changed");
        assert_refused(result, case);
        assert!(!dest.exists(), "{case}: refusal published a snapshot");
    }
}

#[test]
fn staging_holding_only_bridgevm_debris_is_cleared_for_retry() {
    let cases: [&[&str]; 4] = [
        &[],
        &[DISK_NAME],
        &[DISK_NAME, VARS_NAME, ".manifest.json.tmp"],
        &[DISK_NAME, VARS_NAME, MANIFEST_NAME],
    ];
    for (index, names) in cases.into_iter().enumerate() {
        let s = Scratch::new(&format!("admission-debris-{index}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        for name in names {
            fs::write(staging.join(name), b"partial").unwrap();
        }
        let created = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA).unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), created, "{names:?}");
        assert!(!staging.exists(), "{names:?}: staging was not cleared");
    }
}
