use super::*;
use crate::snapshot_pair::create_snapshot;
use crate::snapshot_pair::files::copy_and_sync;
use crate::snapshot_pair::tests::Scratch;
use std::path::PathBuf;

pub(super) struct Fixture {
    pub(super) scratch: Scratch,
    pub(super) root: PathBuf,
    pub(super) stage: PathBuf,
    pub(super) identity: (u64, u64),
    pub(super) manifest: SnapshotManifest,
}

impl Fixture {
    pub(super) fn new(tag: &str) -> Self {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"verified snapshot disk");
        let vars = scratch.write("vars", b"verified snapshot vars");
        let snapshot = scratch.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, "seal-test").unwrap();
        let root = scratch.path("managed");
        let stage = root.join("staging");
        private_directory(&root, true).unwrap();
        private_directory(&stage, true).unwrap();
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            copy_and_sync(&snapshot.join(name), &stage.join(name)).unwrap();
        }
        let id = Identity::from_metadata(&fs::symlink_metadata(&stage).unwrap());
        Self {
            scratch,
            root,
            stage,
            identity: (id.dev, id.ino),
            manifest,
        }
    }

    pub(super) fn seal(&self) -> Record {
        create(&self.root, &self.stage, self.identity, &self.manifest)
            .unwrap()
            .unwrap()
    }

    pub(super) fn assert_media(&self) {
        assert_eq!(
            crate::snapshot_pair::verify_snapshot(&self.stage).unwrap(),
            self.manifest
        );
    }
}

#[test]
fn seal_syncs_record_and_both_directories_before_returning() {
    let fixture = Fixture::new("owned-seal-sync");
    let mut synced = Vec::new();
    let record = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, target| fs::hard_link(source, target),
        |directory| {
            let record = Record::read_pinned(&fixture.root, &fixture.stage)?;
            record.matches_stage(&fixture.stage)?;
            synced.push(directory.to_path_buf());
            sync(directory)
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(synced, [fixture.stage.clone(), fixture.root.clone()]);
    assert_eq!(record.expected_manifest().unwrap(), fixture.manifest);
    fixture.assert_media();
}

#[test]
fn only_precise_unsupported_links_allow_an_unsealed_restore() {
    for (index, code) in [
        libc::ENOTSUP,
        libc::EOPNOTSUPP,
        libc::EIO,
        libc::EPERM,
        libc::EXDEV,
        libc::EEXIST,
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = Fixture::new(&format!("owned-seal-link-{index}"));
        let result = create_using(
            &fixture.root,
            &fixture.stage,
            fixture.identity,
            &fixture.manifest,
            |_, _| Err(io::Error::from_raw_os_error(code)),
            sync,
        );
        if code == libc::ENOTSUP || code == libc::EOPNOTSUPP {
            assert!(result.unwrap().is_none());
        } else {
            assert_eq!(result.unwrap_err().raw_os_error(), Some(code));
        }
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(!fixture.stage.join(PIN_NAME).exists());
        fixture.assert_media();
    }
    let fixture = Fixture::new("owned-seal-kind-only");
    assert!(create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |_, _| Err(io::Error::new(io::ErrorKind::Unsupported, "no errno")),
        sync
    )
    .is_err());
}

#[test]
fn unsupported_link_with_a_pin_never_falls_back() {
    let fixture = Fixture::new("owned-seal-unsupported-pin");
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, target| {
            fs::hard_link(source, target)?;
            Err(io::Error::from_raw_os_error(libc::ENOTSUP))
        },
        sync,
    );
    assert!(result.is_err());
    fixture.assert_media();
}

#[test]
fn directory_sync_failure_aborts_and_only_cleans_owned_metadata() {
    for failed_sync in [0, 1] {
        let fixture = Fixture::new(&format!("owned-seal-sync-error-{failed_sync}"));
        let mut ordinal = 0;
        assert!(create_using(
            &fixture.root,
            &fixture.stage,
            fixture.identity,
            &fixture.manifest,
            |source, target| fs::hard_link(source, target),
            |directory| {
                let current = ordinal;
                ordinal += 1;
                if current == failed_sync {
                    Err(io::Error::other("injected directory sync"))
                } else {
                    sync(directory)
                }
            }
        )
        .is_err());
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(!fixture.stage.join(PIN_NAME).exists());
        fixture.assert_media();
    }
}

#[test]
fn sealing_failure_preserves_replacement_metadata() {
    let fixture = Fixture::new("owned-seal-replaced-record");
    let retained = fixture.scratch.path("original-record");
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, _| {
            fs::rename(source, &retained)?;
            fs::write(source, b"replacement ownership evidence")?;
            Err(io::Error::from_raw_os_error(libc::EIO))
        },
        sync,
    );
    assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EIO));
    assert_eq!(
        fs::read(fixture.root.join(ROOT_NAME)).unwrap(),
        b"replacement ownership evidence"
    );
    assert!(retained.is_file());
    fixture.assert_media();
}

#[test]
fn replacement_members_cannot_inherit_the_original_seal() {
    for name in ["disk.raw", "vars.fd", "manifest.json"] {
        let fixture = Fixture::new(&format!("owned-seal-member-{name}"));
        fixture.seal();
        let path = fixture.stage.join(name);
        let copied = fixture.scratch.path("replacement");
        fs::copy(&path, &copied).unwrap();
        fs::rename(&copied, &path).unwrap();
        let record = Record::read_pinned(&fixture.root, &fixture.stage).unwrap();
        assert!(record.matches_stage(&fixture.stage).is_err());
        fixture.assert_media();
    }
}

#[test]
fn copied_record_and_pin_do_not_acquire_the_record_self_identity() {
    let fixture = Fixture::new("owned-seal-copied-record");
    fixture.seal();
    let root_record = fixture.root.join(ROOT_NAME);
    let copy = fixture.scratch.path("copied-record");
    fs::copy(&root_record, &copy).unwrap();
    fs::remove_file(fixture.stage.join(PIN_NAME)).unwrap();
    fs::rename(&copy, &root_record).unwrap();
    fs::hard_link(&root_record, fixture.stage.join(PIN_NAME)).unwrap();
    assert!(Record::read_pinned(&fixture.root, &fixture.stage).is_err());
    fixture.assert_media();
}

#[test]
fn current_pin_remains_readable_after_writes_change_media_lengths() {
    let fixture = Fixture::new("owned-seal-current-writes");
    fixture.seal();
    let current = fixture.root.join("current");
    fs::rename(&fixture.stage, &current).unwrap();
    fs::write(current.join("disk.raw"), b"guest writes a different length").unwrap();
    let record = Record::read_pinned(&fixture.root, &current).unwrap();
    assert!(record.stage.matches(&current).unwrap());
    assert!(record.matches_stage(&current).is_err());
}

#[test]
fn malformed_or_unbounded_records_never_authorize_recovery() {
    for scenario in [
        "unknown",
        "duplicate",
        "version",
        "array",
        "nested-array",
        "oversize",
    ] {
        let fixture = Fixture::new(&format!("owned-seal-schema-{scenario}"));
        fixture.seal();
        let path = fixture.root.join(ROOT_NAME);
        let text = fs::read_to_string(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let replacement = match scenario {
            "unknown" => {
                value["unknown"] = true.into();
                value.to_string()
            }
            "duplicate" => text.replacen('{', "{\"version\":1,", 1),
            "version" => {
                value["version"] = 2.into();
                value.to_string()
            }
            "array" => "[]".into(),
            "nested-array" => {
                value["root"] = serde_json::json!([value["root"]["dev"], value["root"]["ino"]]);
                value.to_string()
            }
            "oversize" => " ".repeat(256 * 1024 + 1),
            _ => unreachable!(),
        };
        fs::write(&path, replacement).unwrap();
        assert!(
            Record::read_pinned(&fixture.root, &fixture.stage).is_err(),
            "{scenario}"
        );
        fixture.assert_media();
    }
}

#[test]
fn valid_near_limit_manifest_survives_canonical_nested_escaping() {
    let mut fixture = Fixture::new("owned-seal-large-valid-manifest");
    fixture.manifest.vm_id = "\n".repeat(32_000);
    let canonical = fixture.manifest.to_json();
    let compact: serde_json::Value = serde_json::from_str(&canonical).unwrap();
    let source_text = compact.to_string();
    assert!(source_text.len() < 64 * 1024);
    fs::write(fixture.stage.join("manifest.json"), source_text).unwrap();
    let record = fixture.seal();
    assert!(record.encode().unwrap().len() > 200 * 1024);
    assert_eq!(
        Record::read_pinned(&fixture.root, &fixture.stage)
            .unwrap()
            .expected_manifest()
            .unwrap(),
        fixture.manifest
    );
    fixture.assert_media();
}

#[test]
fn unsupported_link_preserves_appledouble_without_admitting_recovery() {
    let fixture = Fixture::new("owned-seal-unsupported-sidecars");
    let names = ["._disk.raw", "._vars.fd", "._manifest.json"];
    for name in names {
        fs::write(fixture.stage.join(name), b"synthetic filesystem metadata").unwrap();
    }
    let mut linked = false;
    let mut synced = Vec::new();
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, target| {
            linked = true;
            assert_eq!(
                fs::metadata(source)?.len(),
                0,
                "the unsupported probe precedes record encoding"
            );
            assert!(!target.exists());
            Err(io::Error::from_raw_os_error(libc::ENOTSUP))
        },
        |directory| {
            synced.push(directory.to_path_buf());
            sync(directory)
        },
    )
    .unwrap();
    assert!(result.is_none());
    assert!(linked);
    assert_eq!(synced, std::slice::from_ref(&fixture.root));
    assert!(!fixture.root.join(ROOT_NAME).exists());
    assert!(!fixture.stage.join(PIN_NAME).exists());
    for name in names {
        assert_eq!(
            fs::read(fixture.stage.join(name)).unwrap(),
            b"synthetic filesystem metadata"
        );
    }
    fixture.assert_media();
}

#[test]
fn supported_link_still_refuses_extra_entries_and_preserves_them() {
    for name in ["._disk.raw", "keep.txt"] {
        let fixture = Fixture::new(&format!("owned-seal-supported-extra-{name}"));
        fs::write(fixture.stage.join(name), b"preserved extra content").unwrap();
        let mut linked = false;
        let result = create_using(
            &fixture.root,
            &fixture.stage,
            fixture.identity,
            &fixture.manifest,
            |source, target| {
                fs::hard_link(source, target)?;
                linked = true;
                Ok(())
            },
            sync,
        );
        assert!(linked, "test did not exercise a supported actual link");
        assert!(result.is_err());
        assert_eq!(
            fs::read(fixture.stage.join(name)).unwrap(),
            b"preserved extra content"
        );
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(!fixture.stage.join(PIN_NAME).exists());
        fixture.assert_media();
    }
}

#[test]
fn replaced_stage_is_refused_before_link_without_pin_pollution() {
    let fixture = Fixture::new("owned-seal-prelink-replaced-stage");
    let original = fixture.scratch.path("original-stage");
    fs::rename(&fixture.stage, &original).unwrap();
    private_directory(&fixture.stage, true).unwrap();
    fs::write(fixture.stage.join("keep.txt"), b"foreign replacement stage").unwrap();
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |_, _| panic!("a replaced stage reached the link probe"),
        sync,
    );
    assert!(result.is_err());
    assert!(!fixture.stage.join(PIN_NAME).exists());
    assert!(!original.join(PIN_NAME).exists());
    assert_eq!(
        fs::read(fixture.stage.join("keep.txt")).unwrap(),
        b"foreign replacement stage"
    );
    assert_eq!(
        crate::snapshot_pair::verify_snapshot(&original).unwrap(),
        fixture.manifest
    );
}

mod recovery {
    //! Recovery from a sealed or unsealed attempt left by an earlier process.

    use super::*;
    use crate::snapshot_pair::managed::{initialize, LockedPair};
    use crate::snapshot_pair::{restore_snapshot, verify_snapshot};
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{symlink, PermissionsExt};

    pub(super) struct Fixture {
        pub(super) scratch: Scratch,
        pub(super) disk: PathBuf,
        pub(super) vars: PathBuf,
        pub(super) snapshot: PathBuf,
        pub(super) root: PathBuf,
        pub(super) stage: PathBuf,
        pub(super) selected: (PathBuf, PathBuf),
        pub(super) old: [Vec<u8>; 2],
        pub(super) source: [Vec<u8>; 3],
        pub(super) manifest: SnapshotManifest,
    }

    pub(super) fn contents(path: &Path) -> [Vec<u8>; 3] {
        ["disk.raw", "vars.fd", "manifest.json"].map(|name| fs::read(path.join(name)).unwrap())
    }

    impl Fixture {
        pub(super) fn new(tag: &str, selected: bool, sealed: bool) -> Self {
            let scratch = Scratch::new(tag);
            let disk = scratch.write("logical-disk", b"original disk");
            let vars = scratch.write("logical-vars", b"original vars");
            let source_disk = scratch.write("source-disk", b"snapshot disk");
            let source_vars = scratch.write("source-vars", b"snapshot vars");
            let snapshot = scratch.path("snapshot");
            let manifest =
                create_snapshot(&source_disk, &source_vars, &snapshot, "source").unwrap();
            let initial = scratch.path("initial");
            if selected {
                create_snapshot(&disk, &vars, &initial, "initial").unwrap();
            }
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            if selected {
                pair.restore(&initial).unwrap();
                let current = pair.paths().unwrap();
                fs::write(&current.0, b"current guest disk").unwrap();
                fs::write(&current.1, b"current guest vars").unwrap();
            }
            let selected = pair.paths().unwrap();
            let old = [
                fs::read(&selected.0).unwrap(),
                fs::read(&selected.1).unwrap(),
            ];
            let root = pair.root.clone();
            initialize(&root, |_| {}).unwrap();
            let stage = root.join("staging");
            if sealed {
                // Use the product producer; dropping its lease models a completed
                // process exit without the publisher's same-attempt error cleanup.
                let (owner, copied) = pair
                    .stage_restore(&snapshot, &manifest, copy_and_sync)
                    .unwrap();
                assert_eq!(copied, manifest);
                drop(owner);
                Record::read_pinned(&root, &stage)
                    .unwrap()
                    .matches_stage(&stage)
                    .unwrap();
            }
            drop(pair);
            let source = contents(&snapshot);
            Self {
                scratch,
                disk,
                vars,
                snapshot,
                root,
                stage,
                selected,
                old,
                source,
                manifest,
            }
        }

        pub(super) fn pair(&self) -> LockedPair {
            LockedPair::open(&self.disk, &self.vars).unwrap()
        }

        pub(super) fn complete_unsealed(&self) {
            private_directory(&self.stage, true).unwrap();
            for name in ["disk.raw", "vars.fd", "manifest.json"] {
                copy_and_sync(&self.snapshot.join(name), &self.stage.join(name)).unwrap();
            }
        }

        pub(super) fn assert_inputs(&self) {
            assert_eq!(contents(&self.snapshot), self.source);
            assert_eq!(fs::read(&self.disk).unwrap(), b"original disk");
            assert_eq!(fs::read(&self.vars).unwrap(), b"original vars");
            assert_eq!(
                [
                    fs::read(&self.selected.0).unwrap(),
                    fs::read(&self.selected.1).unwrap()
                ],
                self.old
            );
        }

        pub(super) fn assert_refused(&self) {
            let before = capture_namespace(&self.scratch.0);
            for _ in 0..2 {
                let mut pair = self.pair();
                assert!(pair.restore(&self.snapshot).is_err());
                assert_eq!(pair.paths().unwrap(), self.selected);
                self.assert_inputs();
                assert_eq!(capture_namespace(&self.scratch.0), before);
            }
        }
    }

    /// Capture synthetic evidence without opening a symlink, FIFO, or other device.
    #[derive(Debug, Eq, PartialEq)]
    enum Contents {
        Directory,
        File(Vec<u8>),
        Symlink(PathBuf),
        Special,
    }

    type Entry = (u64, u64, u32, Contents);

    fn capture_namespace(root: &Path) -> BTreeMap<PathBuf, Entry> {
        fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Entry>) {
            let metadata = fs::symlink_metadata(path).unwrap();
            let contents = if metadata.is_dir() {
                Contents::Directory
            } else if metadata.is_file() {
                Contents::File(fs::read(path).unwrap())
            } else if metadata.file_type().is_symlink() {
                Contents::Symlink(fs::read_link(path).unwrap())
            } else {
                Contents::Special
            };
            result.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                (metadata.dev(), metadata.ino(), metadata.mode(), contents),
            );
            if metadata.is_dir() {
                for entry in fs::read_dir(path).unwrap() {
                    visit(root, &entry.unwrap().path(), result);
                }
            }
        }
        let mut result = BTreeMap::new();
        visit(root, root, &mut result);
        result
    }
    #[test]
    fn sealed_own_stage_allows_ordinary_initial_and_swap_retry() {
        for selected in [false, true] {
            let fixture = Fixture::new(&format!("owned-retry-{selected}"), selected, true);
            fixture.assert_inputs();
            let mut pair = fixture.pair();
            assert_eq!(pair.restore(&fixture.snapshot).unwrap(), fixture.manifest);
            let current = pair.paths().unwrap();
            assert_eq!(fs::read(&current.0).unwrap(), fixture.source[0]);
            assert_eq!(fs::read(&current.1).unwrap(), fixture.source[1]);
            assert_eq!(contents(&fixture.snapshot), fixture.source);
            assert!(!fixture.stage.exists());
            assert!(!fixture.root.join(ROOT_NAME).exists());
        }
    }

    #[test]
    fn authenticated_old_stage_does_not_require_the_same_requested_snapshot() {
        let fixture = Fixture::new("owned-retry-different-source", true, true);
        let inputs = Scratch::new("owned-retry-different-inputs");
        let disk = inputs.write("different-disk", b"different snapshot disk");
        let vars = inputs.write("different-vars", b"different snapshot vars");
        let source = fixture.scratch.path("different");
        let manifest = create_snapshot(&disk, &vars, &source, "different").unwrap();
        let before = contents(&source);
        let mut pair = fixture.pair();
        assert_eq!(pair.restore(&source).unwrap(), manifest);
        let current = pair.paths().unwrap();
        assert_eq!(fs::read(current.0).unwrap(), before[0]);
        assert_eq!(fs::read(current.1).unwrap(), before[1]);
        assert_eq!(contents(&source), before);
        assert_eq!(contents(&fixture.snapshot), fixture.source);
    }

    #[test]
    fn legacy_complete_stage_and_real_foreign_export_stay_intact() {
        for foreign in [false, true] {
            let fixture = Fixture::new(&format!("owned-retry-unsealed-{foreign}"), true, false);
            if foreign {
                let inputs = Scratch::new("owned-retry-foreign-inputs");
                let disk = inputs.write("foreign-disk", b"foreign disk");
                let vars = inputs.write("foreign-vars", b"foreign vars");
                create_snapshot(&disk, &vars, &fixture.stage, "foreign-vm").unwrap();
                // Equivalent private admission to the actual CLI umask077 recipe,
                // without changing the process-wide umask of parallel tests.
                fs::set_permissions(&fixture.stage, fs::Permissions::from_mode(0o700)).unwrap();
            } else {
                fixture.complete_unsealed();
            }
            fixture.assert_refused();
        }
    }

    #[test]
    fn current_record_retirement_tolerates_guest_writes_without_touching_media() {
        let fixture = Fixture::new("owned-current-retirement", false, true);
        let current = fixture.root.join("current");
        crate::snapshot_pair::publish::publish(&fixture.stage, &current).unwrap();
        fs::write(
            current.join("disk.raw"),
            b"new writable current disk with changed length",
        )
        .unwrap();
        fs::write(current.join("vars.fd"), b"new writable vars").unwrap();
        let before = contents(&current);
        assert!(reclaim_sealed(&fixture.root).unwrap());
        assert_eq!(contents(&current), before);
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(current.join(PIN_NAME).is_file());
        let mut pair = fixture.pair();
        pair.restore(&fixture.snapshot).unwrap();
        assert_eq!(
            fs::read(pair.paths().unwrap().0).unwrap(),
            fixture.source[0]
        );
    }
    #[test]
    fn altered_ownership_or_members_preserve_selected_source_and_all_evidence() {
        for selected in [false, true] {
            for scenario in MUTATIONS {
                let fixture = Fixture::new(
                    &format!("owned-refuse-{selected}-{scenario}"),
                    selected,
                    true,
                );
                mutate(&fixture, scenario);
                fixture.assert_refused();
            }
        }
    }

    /// What `fill` leaves when killed during the staged hash of a same-size corrupt
    /// source: admitted without hashing, copied, then sealed with the admitted manifest.
    fn seal_corrupt_copy(fixture: &Fixture) {
        let corrupt = fixture.scratch.path("corrupt");
        fs::create_dir(&corrupt).unwrap();
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            fs::copy(fixture.snapshot.join(name), corrupt.join(name)).unwrap();
        }
        let mut bytes = fixture.source[0].clone();
        bytes[0] ^= 1;
        fs::write(corrupt.join("disk.raw"), bytes).unwrap();
        let admitted = crate::snapshot_pair::inspect_snapshot(&corrupt, false).unwrap();
        private_directory(&fixture.stage, true).unwrap();
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            copy_and_sync(&corrupt.join(name), &fixture.stage.join(name)).unwrap();
        }
        let stage = fs::symlink_metadata(&fixture.stage).unwrap();
        create(
            &fixture.root,
            &fixture.stage,
            (stage.dev(), stage.ino()),
            &admitted,
        )
        .unwrap()
        .unwrap();
    }

    #[test]
    fn sealed_unpublished_copy_is_cleared_even_if_its_contents_are_unusable() {
        for selected in [false, true] {
            // The sealed directory never left the staging name, so it was never published.
            for scenario in [
                "corrupt-source",
                "changed-disk",
                "changed-manifest",
                "unreadable-disk",
            ] {
                let tag = format!("owned-unpublished-{selected}-{scenario}");
                let fixture = Fixture::new(&tag, selected, scenario != "corrupt-source");
                if scenario == "corrupt-source" {
                    seal_corrupt_copy(&fixture);
                } else {
                    mutate(&fixture, scenario);
                }
                let mut pair = fixture.pair();
                assert_eq!(pair.restore(&fixture.snapshot).unwrap(), fixture.manifest);
                let current = pair.paths().unwrap();
                assert_eq!(fs::read(&current.0).unwrap(), fixture.source[0]);
                assert_eq!(fs::read(&current.1).unwrap(), fixture.source[1]);
                assert_eq!(contents(&fixture.snapshot), fixture.source);
                assert!(!fixture.stage.exists(), "{scenario}: staging kept");
                assert!(!fixture.root.join(ROOT_NAME).exists());
            }
            // A member that is not the sealed inode proves nothing: keep everything.
            let fixture = Fixture::new(&format!("owned-unproven-{selected}"), selected, false);
            seal_corrupt_copy(&fixture);
            mutate(&fixture, "replaced-disk");
            fixture.assert_refused();
        }
    }

    #[test]
    fn exchanged_old_generation_never_inherits_the_new_stage_record() {
        let fixture = Fixture::new("owned-refuse-exchanged-old", true, true);
        let current = fixture.root.join("current");
        let parked = fixture.scratch.path("parked-current");
        let old = contents(&current);
        // The three renames model the completed exchange before its parent-sync
        // boundary; both original directory identities are retained.
        fs::rename(&current, &parked).unwrap();
        fs::rename(&fixture.stage, &current).unwrap();
        fs::rename(&parked, &fixture.stage).unwrap();
        for partial in [false, true] {
            if partial {
                fs::remove_file(fixture.stage.join("disk.raw")).unwrap();
            }
            let mut pair = fixture.pair();
            assert!(pair.restore(&fixture.snapshot).is_err());
            assert_eq!(contents(&current), fixture.source);
            assert_eq!(contents(&fixture.snapshot), fixture.source);
            assert_eq!(fs::read(fixture.stage.join("vars.fd")).unwrap(), old[1]);
            assert_eq!(
                fs::read(fixture.stage.join("manifest.json")).unwrap(),
                old[2]
            );
            if partial {
                assert!(!fixture.stage.join("disk.raw").exists());
            } else {
                assert_eq!(fs::read(fixture.stage.join("disk.raw")).unwrap(), old[0]);
            }
        }
    }
    const MUTATIONS: &[&str] = &[
        "missing-record",
        "missing-pin",
        "torn-record",
        "copied-record",
        "copied-pin",
        "copied-both",
        "replaced-stage",
        "replaced-disk",
        "replaced-vars",
        "replaced-manifest",
        "unknown-entry",
        "missing-member",
        "record-symlink",
        "pin-symlink",
        "disk-symlink",
        "record-fifo",
        "pin-fifo",
        "disk-fifo",
    ];

    fn replace_copy(path: &Path, temporary: &Path) {
        fs::copy(path, temporary).unwrap();
        fs::rename(temporary, path).unwrap();
    }

    fn fifo(path: &Path) {
        fs::remove_file(path).unwrap();
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: the CString is live and NUL-terminated; only this test owns its
        // tiny synthetic directory. A read must reject this FIFO without blocking.
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    }

    fn mutate(fixture: &Fixture, scenario: &str) {
        let record = fixture.root.join(ROOT_NAME);
        let pin = fixture.stage.join(PIN_NAME);
        let temporary = fixture.scratch.path("replacement");
        let retained = fixture.scratch.path("retained-original");
        match scenario {
            "missing-record" => fs::remove_file(record).unwrap(),
            "missing-pin" => fs::remove_file(pin).unwrap(),
            "torn-record" => fs::write(record, b"{").unwrap(),
            "copied-record" => replace_copy(&record, &temporary),
            "copied-pin" => replace_copy(&pin, &temporary),
            "copied-both" => {
                replace_copy(&record, &temporary);
                fs::remove_file(&pin).unwrap();
                fs::hard_link(&record, &pin).unwrap();
            }
            "replaced-stage" => {
                fs::rename(&fixture.stage, &retained).unwrap();
                private_directory(&fixture.stage, true).unwrap();
                for name in ["disk.raw", "vars.fd", "manifest.json"] {
                    fs::copy(retained.join(name), fixture.stage.join(name)).unwrap();
                }
                // Keep the genuine two-link token, so directory identity is the
                // rejecting boundary rather than an incidental link-count error.
                fs::rename(retained.join(PIN_NAME), &pin).unwrap();
            }
            "replaced-disk" => replace_copy(&fixture.stage.join("disk.raw"), &temporary),
            "replaced-vars" => replace_copy(&fixture.stage.join("vars.fd"), &temporary),
            "replaced-manifest" => replace_copy(&fixture.stage.join("manifest.json"), &temporary),
            "changed-disk" => {
                let path = fixture.stage.join("disk.raw");
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] ^= 1;
                fs::write(path, bytes).unwrap();
            }
            "changed-manifest" => {
                let path = fixture.stage.join("manifest.json");
                let text = fs::read_to_string(&path)
                    .unwrap()
                    .replace("\"source\"", "\"tamper\"");
                fs::write(path, text).unwrap();
            }
            "unreadable-disk" => fs::set_permissions(
                fixture.stage.join("disk.raw"),
                fs::Permissions::from_mode(0o000),
            )
            .unwrap(),
            "unknown-entry" => {
                fs::write(fixture.stage.join("keep.txt"), b"unrelated bytes").unwrap()
            }
            "missing-member" => fs::remove_file(fixture.stage.join("vars.fd")).unwrap(),
            "record-symlink" => {
                fs::rename(&record, &retained).unwrap();
                symlink(&retained, record).unwrap();
            }
            "pin-symlink" => {
                fs::rename(&pin, &retained).unwrap();
                symlink(&record, pin).unwrap();
            }
            "disk-symlink" => {
                let disk = fixture.stage.join("disk.raw");
                fs::rename(&disk, &retained).unwrap();
                symlink(&retained, disk).unwrap();
            }
            "record-fifo" => fifo(&record),
            "pin-fifo" => fifo(&pin),
            "disk-fifo" => fifo(&fixture.stage.join("disk.raw")),
            _ => panic!("unknown ownership mutation {scenario}"),
        }
    }
    #[test]
    fn repeated_restore_guest_writes_and_export_use_current_beside_retained_pin() {
        let fixture = Fixture::new("owned-current-runtime", false, false);
        for generation in 0..3 {
            restore_snapshot(&fixture.snapshot, &fixture.disk, &fixture.vars).unwrap();
            let pair = fixture.pair();
            let (disk, vars) = pair.paths().unwrap();
            assert_eq!(fs::read(&disk).unwrap(), fixture.source[0]);
            assert_eq!(fs::read(&vars).unwrap(), fixture.source[1]);
            let current = disk.parent().unwrap();
            assert!(current.join(PIN_NAME).is_file());
            assert!(!fixture.root.join(ROOT_NAME).exists());
            let written_disk = format!("guest disk after restore {generation}").into_bytes();
            let written_vars = format!("guest vars after restore {generation}").into_bytes();
            fs::write(&disk, &written_disk).unwrap();
            fs::write(&vars, &written_vars).unwrap();
            drop(pair);
            let export = fixture.scratch.path(&format!("export-{generation}"));
            let manifest =
                create_snapshot(&fixture.disk, &fixture.vars, &export, "current").unwrap();
            assert_eq!(verify_snapshot(&export).unwrap(), manifest);
            assert_eq!(fs::read(export.join("disk.raw")).unwrap(), written_disk);
            assert_eq!(fs::read(export.join("vars.fd")).unwrap(), written_vars);
            assert!(!export.join(PIN_NAME).exists());
            assert_eq!(fs::read(&fixture.disk).unwrap(), b"original disk");
            assert_eq!(fs::read(&fixture.vars).unwrap(), b"original vars");
            assert_eq!(contents(&fixture.snapshot), fixture.source);
        }
    }

    #[test]
    fn foreign_export_cannot_replace_the_sealed_restore_attempt() {
        let fixture = Fixture::new("owned-stage-export-refusal", false, true);
        let inputs = Scratch::new("owned-stage-foreign-inputs");
        let disk = inputs.write("foreign-disk", b"foreign disk");
        let vars = inputs.write("foreign-vars", b"foreign vars");
        let admitted = inputs.path("valid-export");
        create_snapshot(&disk, &vars, &admitted, "foreign").unwrap();
        let before = capture_namespace(&fixture.scratch.0);
        assert!(create_snapshot(&disk, &vars, &fixture.stage, "foreign").is_err());
        assert_eq!(capture_namespace(&fixture.scratch.0), before);
        assert_eq!(fs::read(&disk).unwrap(), b"foreign disk");
        assert_eq!(fs::read(&vars).unwrap(), b"foreign vars");
        fixture.assert_inputs();
        let mut pair = fixture.pair();
        pair.restore(&fixture.snapshot).unwrap();
    }
    #[test]
    fn partial_exchanged_legacy_generation_is_preserved_across_retries() {
        let fixture = Fixture::new("owned-legacy-exchange", true, true);
        let current = fixture.root.join("current");
        // A generation created before ownership seals had no retained pin.
        fs::remove_file(current.join(PIN_NAME)).unwrap();
        let parked = fixture.scratch.path("legacy-current");
        let old = contents(&current);
        fs::rename(&current, &parked).unwrap();
        fs::rename(&fixture.stage, &current).unwrap();
        fs::rename(&parked, &fixture.stage).unwrap();
        // Model the existing publication cleanup interrupted after these members.
        fs::remove_file(fixture.stage.join("disk.raw")).unwrap();
        fs::remove_file(fixture.stage.join("manifest.json")).unwrap();
        let before = capture_namespace(&fixture.scratch.0);
        for _ in 0..2 {
            let mut pair = fixture.pair();
            let result = pair.restore(&fixture.snapshot);
            assert_eq!(
                fs::read(fixture.stage.join("vars.fd")).ok(),
                Some(old[1].clone())
            );
            assert!(result.is_err());
            assert_eq!(capture_namespace(&fixture.scratch.0), before);
            assert_eq!(contents(&current), fixture.source);
            assert_eq!(fs::read(fixture.stage.join("vars.fd")).unwrap(), old[1]);
        }
        let preserved = fixture.scratch.path("preserved-legacy-current");
        fs::rename(&fixture.stage, &preserved).unwrap();
        fixture.pair().restore(&fixture.snapshot).unwrap();
        assert_eq!(fs::read(preserved.join("vars.fd")).unwrap(), old[1]);
        assert!(!fixture.root.join(ROOT_NAME).exists());
    }
    #[test]
    fn successful_publisher_with_old_debris_keeps_ownership_for_later_retry() {
        let fixture = Fixture::new("owned-retirement-old-debris", true, false);
        let current = fixture.root.join("current");
        fs::remove_file(current.join(PIN_NAME)).unwrap();
        let old = contents(&current);
        let parked = fixture.scratch.path("legacy-current");
        let mut pair = fixture.pair();
        let published = pair.restore_using(&fixture.snapshot, |stage, destination| {
            fs::rename(destination, &parked)?;
            fs::rename(stage, destination)?;
            fs::rename(&parked, stage)?;
            fs::remove_file(stage.join("disk.raw"))?;
            fs::remove_file(stage.join("manifest.json"))?;
            // Publication may report success after best-effort old-generation
            // removal fails. The ownership retirement must preserve that fact.
            Ok(())
        });
        assert_eq!(contents(&current), fixture.source);
        assert_eq!(fs::read(fixture.stage.join("vars.fd")).unwrap(), old[1]);
        drop(pair);
        let before = capture_namespace(&fixture.scratch.0);
        let mut pair = fixture.pair();
        let retried = pair.restore(&fixture.snapshot);
        assert_eq!(
            fs::read(fixture.stage.join("vars.fd")).ok(),
            Some(old[1].clone())
        );
        assert!(published.is_err());
        assert!(retried.is_err());
        assert_eq!(capture_namespace(&fixture.scratch.0), before);
        assert!(fixture.root.join(ROOT_NAME).is_file());
        assert!(published
            .unwrap_err()
            .to_string()
            .contains("it was left intact"));
        drop(pair);
        let preserved = fixture.scratch.path("preserved-legacy-current");
        fs::rename(&fixture.stage, &preserved).unwrap();
        fixture.pair().restore(&fixture.snapshot).unwrap();
        assert_eq!(fs::read(preserved.join("vars.fd")).unwrap(), old[1]);
        assert!(!fixture.root.join(ROOT_NAME).exists());
    }
}
