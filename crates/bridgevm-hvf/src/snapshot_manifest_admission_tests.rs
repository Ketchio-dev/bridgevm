use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{
    create_snapshot, restore_snapshot, staging_path, verify_snapshot, SnapshotError, DISK_NAME,
    MANIFEST_NAME, VARS_NAME,
};
use std::{fs, io};

#[test]
fn malformed_manifests_cannot_authorize_replacement_or_restore() {
    for case in [
        "version-exponent",
        "missing-punctuation",
        "duplicate-field",
        "trailing-document",
    ] {
        let scratch = Scratch::new(&format!("manifest-admission-{case}"));
        let disk = scratch.write("disk", b"snapshot disk");
        let vars = scratch.write("vars", b"snapshot vars");
        let destination = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &destination, "vm", false, QUOTA).unwrap();
        let text = fs::read_to_string(destination.join(MANIFEST_NAME)).unwrap();
        let malformed = match case {
            "version-exponent" => text.replace("\"format_version\": 1", "\"format_version\": 1e2"),
            "missing-punctuation" => text
                .replace("{\n", "")
                .replace("\n}\n", "\n")
                .replace(",\n", "\n"),
            "duplicate-field" => text.replacen('{', "{\"vm_id\":\"other\",", 1),
            "trailing-document" => text + "{}",
            _ => unreachable!(),
        };
        fs::write(destination.join(MANIFEST_NAME), &malformed).unwrap();
        let before = [DISK_NAME, VARS_NAME, MANIFEST_NAME]
            .map(|name| fs::read(destination.join(name)).unwrap());
        fs::write(&disk, b"keep live disk").unwrap();
        fs::write(&vars, b"keep live vars").unwrap();

        assert!(matches!(
            verify_snapshot(&destination),
            Err(SnapshotError::BadManifest(_))
        ));
        let replacement = create_snapshot(&disk, &vars, &destination, "new", false, QUOTA);
        let Err(SnapshotError::Io(error)) = replacement else {
            panic!("{case}: replacement accepted malformed snapshot: {replacement:?}");
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(error.to_string().contains("left intact"));
        assert!(!staging_path(&destination).exists());
        assert!(matches!(
            restore_snapshot(&destination, &disk, &vars, false),
            Err(SnapshotError::BadManifest(_))
        ));
        let after = [DISK_NAME, VARS_NAME, MANIFEST_NAME]
            .map(|name| fs::read(destination.join(name)).unwrap());
        assert_eq!(after, before, "{case}: malformed destination was changed");
        assert_eq!(fs::read(&disk).unwrap(), b"keep live disk");
        assert_eq!(fs::read(&vars).unwrap(), b"keep live vars");
        assert!(!fs::read_dir(&scratch.0).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".bridgevm-pair-")));
    }
}
