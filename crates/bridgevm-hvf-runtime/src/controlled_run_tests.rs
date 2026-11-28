use super::*;
use crate::{owned_control::test_support::Fixture, prepare, DeviceSurfaces};
use bridgevm_hvf::snapshot_pair::{create_snapshot, restore_snapshot};
use std::cell::Cell;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn logged_surfaces(evidence_dir: std::path::PathBuf) -> DeviceSurfaces {
    DeviceSurfaces {
        evidence_dir,
        display_export_ms: 100,
        input_control: None,
        virtio_gpu_3d: None,
        virtio_gpu_2d: false,
        aggressive_performance: false,
        nvme_buffered_io: false,
        nvme_durable_flush: false,
        guest_resolution: None,
        clipboard_sync: false,
        share: None,
        virtio_net: false,
        net_forwards: Vec::new(),
        hda_audio: false,
        hda_microphone: false,
        agent_update: None,
        guest_tools: None,
        host_diagnostic_stop: false,
    }
}

/// A helper that fails, or is stopped by Force Stop, leaves no stop save of
/// its own: the runtime syncs the media it used, the restored generation
/// here. That file is made unreadable so the sync's failure shows the file it
/// opened, reported with the helper's own line.
#[test]
fn failed_or_cancelled_helper_still_syncs_its_selected_media() {
    for cancel in [false, true] {
        let fixture = Fixture::new();
        let (disk, vars) = (fixture.root.join("disk.raw"), fixture.root.join("vars.fd"));
        let snapshot = fixture.root.join("saved.snapshot");
        create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        restore_snapshot(&snapshot, &disk, &vars).unwrap();
        let (selected, _) = LockedPair::open(&disk, &vars).unwrap().paths().unwrap();
        assert_ne!(selected, fs::canonicalize(&disk).unwrap());
        fs::set_permissions(&selected, fs::Permissions::from_mode(0o000)).unwrap();
        let started = fixture.root.join("started");
        let body = if cancel {
            format!("touch '{}'; exec sleep 30", started.display())
        } else {
            "exit 1".into()
        };
        let mut launch = fixture.launch(fixture.script("helper.sh", &body));
        let evidence = fixture.root.join("evidence");
        fs::create_dir(&evidence).unwrap();
        launch.surfaces = Some(logged_surfaces(evidence.clone()));
        let prepared = prepare(fixture.manifest(), "sync fixture").unwrap();
        let requested = || cancel && started.exists();
        let control = RuntimeControl::new(&requested, &|_| panic!("unexpected uncertainty"));
        let receipt = fixture.root.join("receipt");
        let outcome = run_prepared_vm(&prepared, &launch, &receipt, 1, &control);
        fs::set_permissions(&selected, fs::Permissions::from_mode(0o600)).unwrap();
        match outcome {
            Ok(run) => assert!(cancel && run.termination == RunTermination::Cancelled),
            Err(error) => assert!(!cancel && error.to_string().contains("VM helper failed")),
        }
        let log = fs::read_to_string(evidence.join("run.log")).unwrap();
        let reported = log.lines().filter(|l| *l == "BVSTORAGE final-save-failed");
        assert_eq!(reported.count(), 1, "cancel={cancel}: {log:?}");
        assert!(!receipt.exists());
    }
}

#[test]
fn pulse_cancellation_during_first_generation_never_launches_another_generation() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("generations");
    let helper = fixture.script(
        "reset.sh",
        &format!(
            "printf '%s\\n' \"$BRIDGEVM_RESET_GENERATION\" >> '{}'; exit 42",
            marker.display()
        ),
    );
    let prepared = prepare(fixture.manifest(), "cancel reset fixture").unwrap();
    let pulsed = Cell::new(false);
    let requested =
        || std::fs::read_to_string(&marker).ok().as_deref() == Some("0\n") && !pulsed.replace(true);
    let control = RuntimeControl::new(&requested, &|_| panic!("unexpected uncertainty"));
    let outcome = run_prepared_vm(
        &prepared,
        &fixture.launch(helper),
        &fixture.root.join("receipt"),
        4,
        &control,
    )
    .unwrap();
    assert_eq!(outcome.termination, RunTermination::Cancelled);
    assert!(control.is_cancelled());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "0\n");
    assert!(!fixture.root.join("receipt").exists());
    assert_eq!(prepared.generation().stamp().value(), 0);
    assert!(prepare(fixture.manifest(), "competitor").is_err());
    drop(prepared);
    assert!(prepare(fixture.manifest(), "after cleanup").is_ok());
}

#[test]
fn cancellation_after_flushed_reset_blocks_the_next_generation() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("generations");
    let helper = fixture.script(
        "reset.sh",
        &format!(
            "printf '%s\\n' \"$BRIDGEVM_RESET_GENERATION\" >> '{}'; exit 42",
            marker.display()
        ),
    );
    let prepared = prepare(fixture.manifest(), "post-reset fixture").unwrap();
    let pulsed = Cell::new(false);
    let requested = || prepared.generation().stamp().value() > 0 && !pulsed.replace(true);
    let control = RuntimeControl::new(&requested, &|_| panic!("unexpected uncertainty"));
    let receipt = fixture.root.join("receipt");
    let outcome =
        run_prepared_vm(&prepared, &fixture.launch(helper), &receipt, 4, &control).unwrap();
    assert_eq!(outcome.termination, RunTermination::Cancelled);
    assert!(control.is_cancelled());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "0\n");
    assert!(std::fs::read_to_string(receipt)
        .unwrap()
        .contains("generation: 0"));
    assert_eq!(outcome.cycles.len(), 1);
    assert_eq!(prepared.generation().stamp().value(), 1);
    assert!(prepare(fixture.manifest(), "competitor").is_err());
}

#[test]
fn cancellation_before_first_spawn_preserves_media_and_has_no_helper_effect() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("unexpected-child");
    let helper = fixture.script("never.sh", &format!("touch '{}'", marker.display()));
    let prepared = prepare(fixture.manifest(), "pre-cancel fixture").unwrap();
    let outcome = run_prepared_vm(
        &prepared,
        &fixture.launch(helper),
        &fixture.root.join("receipt"),
        4,
        &RuntimeControl::new(&|| true, &|_| panic!("unexpected uncertainty")),
    )
    .unwrap();
    assert_eq!(outcome.termination, RunTermination::Cancelled);
    assert!(outcome.cycles.is_empty());
    assert!(!marker.exists());
    assert!(prepare(fixture.manifest(), "competitor").is_err());
}
