use super::*;
use crate::snapshot_pair::{restore_snapshot, verify_snapshot};

#[test]
fn repeated_restore_guest_writes_and_export_use_current_beside_retained_pin() {
    let fixture = Fixture::new("owned-current-runtime", false, false);
    for generation in 0..3 {
        restore_snapshot(&fixture.snapshot, &fixture.disk, &fixture.vars, false).unwrap();
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
        let manifest = create_snapshot(
            &fixture.disk,
            &fixture.vars,
            &export,
            "current",
            false,
            1024,
        )
        .unwrap();
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
    create_snapshot(&disk, &vars, &admitted, "foreign", false, 1024).unwrap();
    let before = fixture::namespace::capture(&fixture.scratch.0);
    assert!(create_snapshot(&disk, &vars, &fixture.stage, "foreign", false, 1024).is_err());
    assert_eq!(fixture::namespace::capture(&fixture.scratch.0), before);
    assert_eq!(fs::read(&disk).unwrap(), b"foreign disk");
    assert_eq!(fs::read(&vars).unwrap(), b"foreign vars");
    fixture.assert_inputs();
    let mut pair = fixture.pair();
    pair.restore(&fixture.snapshot).unwrap();
}
