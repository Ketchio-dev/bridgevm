use hvf_runner::task5af::{run_task5af, Task5afRequest};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs as unix_fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct TempPath {
    path: PathBuf,
}

impl TempPath {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bridgevm-task5af-{name}-{}-{nonce}",
            std::process::id()
        ));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if self.path.is_dir() {
            let _ = fs::remove_dir_all(&self.path);
        } else {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn request(evidence_dir: PathBuf) -> Task5afRequest {
    Task5afRequest {
        evidence_dir,
        row: "baseline".to_string(),
        reuse_ledger: None,
        allow_live: false,
        live_env_allowed: false,
        command: vec!["task5af_live_row".to_string(), "--row".to_string()],
        env: vec![(
            "BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE".to_string(),
            "<unset>".to_string(),
        )],
    }
}

fn read_json(path: &Path) -> Value {
    let content = fs::read_to_string(path).expect("json file should be readable");
    serde_json::from_str(&content).expect("json should parse")
}

#[test]
fn task5af_safe_preflight_writes_manifest_ledger_and_row_files() {
    // Given: a fresh evidence directory for the baseline Task 5af row.
    let evidence_dir = TempPath::new("happy");

    // When: the safe preflight runner surface is executed.
    let outcome =
        run_task5af(request(evidence_dir.path().to_path_buf())).expect("task5af should run");

    // Then: the binary-observable evidence contract is materialized.
    assert_eq!(outcome.classification, "safe_preflight");
    let manifest = read_json(&evidence_dir.path().join("manifest.json"));
    assert_eq!(manifest["schema"], "bridgevm.task5af.live_row.v1");
    assert_eq!(manifest["task"], "5af");
    assert_eq!(manifest["run_id"], outcome.run_id);
    assert_eq!(manifest["row_count"], 1);
    assert_eq!(manifest["started_row_count"], 1);
    assert_eq!(manifest["finished_row_count"], 1);
    assert_eq!(manifest["live_hvf_started"], false);
    assert_eq!(manifest["qemu_started"], false);
    assert_eq!(
        manifest["claims_allowed"]
            .as_array()
            .expect("claims_allowed should be an array")
            .len(),
        0
    );
    assert_eq!(manifest["row"], "baseline");
    assert_eq!(manifest["classification"], "safe_preflight");
    assert_eq!(manifest["paths"]["stdout"], "rows/baseline/stdout.txt");
    assert_eq!(manifest["paths"]["stderr"], "rows/baseline/stderr.txt");

    let ledger = fs::read_to_string(evidence_dir.path().join("ledger.jsonl"))
        .expect("ledger should be readable");
    let events: Vec<Value> = ledger
        .lines()
        .map(|line| serde_json::from_str(line).expect("ledger line should parse"))
        .collect();
    let names: Vec<&str> = events
        .iter()
        .map(|event| event["event"].as_str().expect("event should be a string"))
        .collect();
    assert_eq!(names, ["row_planned", "row_started", "row_finished"]);

    let stdout = fs::read_to_string(evidence_dir.path().join("rows/baseline/stdout.txt"))
        .expect("stdout evidence should be readable");
    let stderr = fs::read_to_string(evidence_dir.path().join("rows/baseline/stderr.txt"))
        .expect("stderr evidence should be readable");
    assert!(stdout.contains("safe preflight"));
    assert!(stderr.is_empty());
}

#[test]
fn task5af_rejects_symlinked_rows_ancestor_without_writing_outside_evidence_root() {
    // Given: an evidence root whose rows ancestor is a symlink to an outside directory.
    let evidence_dir = TempPath::new("symlink-evidence");
    let outside_dir = TempPath::new("symlink-outside");
    fs::create_dir_all(evidence_dir.path()).expect("evidence root should be creatable");
    fs::create_dir_all(outside_dir.path()).expect("outside root should be creatable");
    unix_fs::symlink(outside_dir.path(), evidence_dir.path().join("rows"))
        .expect("rows symlink should be creatable");

    // When: the safe preflight runner is asked to write row evidence.
    let error =
        run_task5af(request(evidence_dir.path().to_path_buf())).expect_err("symlink should fail");

    // Then: the request is rejected and stdout/stderr are not created outside the evidence root.
    assert!(error.to_string().contains("symlink"));
    assert!(!outside_dir.path().join("baseline/stdout.txt").exists());
    assert!(!outside_dir.path().join("baseline/stderr.txt").exists());
}

#[test]
fn task5af_reuse_ledger_is_rejected_with_receipt_and_original_ledger_unchanged() {
    // Given: an existing ledger that must not be appended to by this first slice.
    let evidence_dir = TempPath::new("reuse");
    let existing_ledger = TempPath::new("existing-ledger.jsonl");
    fs::write(existing_ledger.path(), "{\"event\":\"old\"}\n").expect("seed ledger should write");
    let before = fs::read_to_string(existing_ledger.path()).expect("seed ledger should read");
    let mut task_request = request(evidence_dir.path().to_path_buf());
    task_request.reuse_ledger = Some(existing_ledger.path().to_path_buf());

    // When: reuse-ledger is provided.
    let error = run_task5af(task_request).expect_err("reuse-ledger should fail closed");

    // Then: a rejection receipt is written under the new evidence dir and the old ledger is unchanged.
    assert!(error.to_string().contains("--reuse-ledger"));
    let after = fs::read_to_string(existing_ledger.path()).expect("seed ledger should still read");
    assert_eq!(after, before);
    let receipt = read_json(&evidence_dir.path().join("reuse-ledger-rejection.json"));
    assert_eq!(
        receipt["classification"],
        "rejected_reuse_ledger_unsupported"
    );
    assert_eq!(receipt["sha256_before"], receipt["sha256_after"]);
    assert_eq!(
        receipt["reuse_ledger_path"],
        existing_ledger.path().display().to_string()
    );
}

#[test]
fn task5af_reuse_ledger_rejects_symlinked_evidence_root_without_writing_receipt_outside() {
    // Given: reuse-ledger failure evidence would be written through a symlinked evidence root.
    let evidence_link = TempPath::new("reuse-symlink-evidence");
    let outside_dir = TempPath::new("reuse-symlink-outside");
    let existing_ledger = TempPath::new("reuse-symlink-existing-ledger.jsonl");
    fs::create_dir_all(outside_dir.path()).expect("outside root should be creatable");
    fs::write(existing_ledger.path(), "{\"event\":\"old\"}\n").expect("seed ledger should write");
    unix_fs::symlink(outside_dir.path(), evidence_link.path())
        .expect("evidence symlink should be creatable");
    let mut task_request = request(evidence_link.path().to_path_buf());
    task_request.reuse_ledger = Some(existing_ledger.path().to_path_buf());

    // When: reuse-ledger is rejected.
    let error = run_task5af(task_request).expect_err("symlinked evidence root should fail");

    // Then: no rejection receipt is written outside the requested evidence root.
    assert!(error.to_string().contains("symlink"));
    assert!(!outside_dir
        .path()
        .join("reuse-ledger-rejection.json")
        .exists());
}

#[test]
fn task5af_allow_live_requires_explicit_environment_gate() {
    // Given: a live request without the BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE=1 gate.
    let evidence_dir = TempPath::new("live-env");
    let mut task_request = request(evidence_dir.path().to_path_buf());
    task_request.allow_live = true;
    task_request.live_env_allowed = false;

    // When: the runner validates live mode.
    let error = run_task5af(task_request).expect_err("live without env gate should fail");

    // Then: it fails closed before creating the normal HVF evidence artifacts.
    assert!(error
        .to_string()
        .contains("BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE=1"));
    assert!(!evidence_dir.path().join("manifest.json").exists());
}
