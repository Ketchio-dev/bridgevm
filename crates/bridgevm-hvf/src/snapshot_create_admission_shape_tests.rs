use super::*;

/// Declare each member's lstat size, so only its file type can refuse it.
fn declare_actual_sizes(dest: &Path, old: &SnapshotManifest) {
    let size = |name| fs::symlink_metadata(dest.join(name)).unwrap().len();
    let forged = SnapshotManifest {
        disk_bytes: size(DISK_NAME),
        vars_bytes: size(VARS_NAME),
        ..old.clone()
    };
    fs::write(dest.join(MANIFEST_NAME), forged.to_json()).unwrap();
}

#[test]
fn inadmissible_snapshot_shapes_are_refused_before_staging() {
    for case in [
        "missing-member",
        "subdirectory",
        "unparseable-manifest",
        "unsupported-manifest",
        "oversized-manifest",
        "disk-size-mismatch",
        "vars-size-mismatch",
        "linked-member",
        "directory-member",
        "unrelated-names",
    ] {
        let s = Scratch::new(&format!("admission-shape-{case}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let old = create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
        let elsewhere = s.write("elsewhere", b"new disk");
        match case {
            "missing-member" => fs::remove_file(dest.join(VARS_NAME)).unwrap(),
            "subdirectory" => {
                fs::create_dir(dest.join("nested")).unwrap();
                fs::write(dest.join("nested/keep.txt"), b"nested data").unwrap();
            }
            "unparseable-manifest" => fs::write(dest.join(MANIFEST_NAME), b"not json").unwrap(),
            "unsupported-manifest" => {
                let text = old
                    .to_json()
                    .replace("\"format_version\": 1", "\"format_version\": 2");
                fs::write(dest.join(MANIFEST_NAME), text).unwrap();
            }
            // Still parses: only the size limit refuses it.
            "oversized-manifest" => {
                let text = old.to_json() + &" ".repeat(64 * 1024);
                fs::write(dest.join(MANIFEST_NAME), text).unwrap();
            }
            "disk-size-mismatch" => fs::write(dest.join(DISK_NAME), b"longer disk").unwrap(),
            "vars-size-mismatch" => fs::write(dest.join(VARS_NAME), b"v").unwrap(),
            "linked-member" => {
                fs::remove_file(dest.join(DISK_NAME)).unwrap();
                symlink(&elsewhere, dest.join(DISK_NAME)).unwrap();
                declare_actual_sizes(&dest, &old);
            }
            "directory-member" => {
                fs::remove_file(dest.join(VARS_NAME)).unwrap();
                fs::create_dir(dest.join(VARS_NAME)).unwrap();
                fs::write(dest.join(VARS_NAME).join("keep.txt"), b"nested data").unwrap();
                declare_actual_sizes(&dest, &old);
            }
            "unrelated-names" => {
                for name in [DISK_NAME, VARS_NAME, MANIFEST_NAME] {
                    fs::rename(dest.join(name), dest.join(format!("{name}.bak"))).unwrap();
                }
            }
            _ => unreachable!(),
        }
        let before = (tree(&dest), tree(&elsewhere));
        let result = create_uncopied(&disk, &vars, &dest, "new");
        assert_eq!((tree(&dest), tree(&elsewhere)), before, "{case}: changed");
        assert_refused(result, case);
        assert!(
            !staging_path(&dest).exists(),
            "{case}: refusal left staging"
        );
    }
}

/// exFAT keeps a written file's extended attributes in a "._" AppleDouble file
/// beside it, and its directory exchange fails with ENOTSUP, so a snapshot there
/// cannot be replaced in place anyway.
#[test]
fn a_snapshot_with_apple_double_companions_is_refused_as_not_a_snapshot() {
    let s = Scratch::new("admission-apple-double");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
    s.write("snapshot/._disk.raw", b"\0\x05\x16\x07 AppleDouble");
    let before = tree(&dest);
    let result = create_uncopied(&disk, &vars, &dest, "new");
    assert_eq!(tree(&dest), before);
    let Err(SnapshotError::Io(error)) = result else {
        panic!("expected a refusal, got {result:?}");
    };
    let expected = "contains \"._disk.raw\", which is not a snapshot file; it was left intact";
    assert!(error.to_string().contains(expected), "{error}");
}
