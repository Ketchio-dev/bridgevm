use crate::task5af_evidence::reject_symlinked_evidence_component;
use crate::task5af_evidence::sha256_file;
use crate::task5af_evidence::write_new_file;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const SCHEMA: &str = "bridgevm.task5af.live_row.v1";
const TASK: &str = "5af";
const LIVE_ENV_GATE: &str = "BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE=1";
const SAFE_PREFLIGHT_CLASSIFICATION: &str = "safe_preflight";
const REUSE_LEDGER_REJECTION: &str = "rejected_reuse_ledger_unsupported";

static NEXT_RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task5afRequest {
    pub evidence_dir: PathBuf,
    pub row: String,
    pub reuse_ledger: Option<PathBuf>,
    pub allow_live: bool,
    pub live_env_allowed: bool,
    pub command: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task5afOutcome {
    pub run_id: String,
    pub classification: String,
}

pub fn run_task5af(request: Task5afRequest) -> Result<Task5afOutcome> {
    if request.allow_live && !request.live_env_allowed {
        bail!("--allow-live requires {LIVE_ENV_GATE}; refusing to start live HVF");
    }

    let row = RowName::parse(&request.row)?;
    let run_id = fresh_run_id()?;

    if let Some(reuse_ledger) = request.reuse_ledger.as_ref() {
        return reject_reused_ledger(&request, &row, &run_id, reuse_ledger);
    }

    write_safe_preflight(request, &row, run_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RowName(String);

impl RowName {
    fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            bail!("--row must not be empty");
        }
        if trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') {
            bail!("--row must be a single safe path segment");
        }
        Ok(Self(trimmed.to_string()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema: &'static str,
    task: &'static str,
    run_id: String,
    row_count: u32,
    started_row_count: u32,
    finished_row_count: u32,
    live_hvf_started: bool,
    qemu_started: bool,
    claims_allowed: Vec<String>,
    row: String,
    command: Vec<String>,
    env: BTreeMap<String, String>,
    paths: EvidencePaths,
    classification: &'static str,
}

#[derive(Debug, Serialize)]
struct EvidencePaths {
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct LedgerEvent<'a> {
    schema: &'static str,
    task: &'static str,
    run_id: &'a str,
    row: &'a str,
    event: &'static str,
    classification: &'static str,
    live_hvf_started: bool,
    qemu_started: bool,
}

#[derive(Debug, Serialize)]
struct ReuseLedgerRejection {
    schema: &'static str,
    task: &'static str,
    run_id: String,
    row: String,
    classification: &'static str,
    reuse_ledger_path: String,
    sha256_before: String,
    sha256_after: String,
    live_hvf_started: bool,
    qemu_started: bool,
}

fn write_safe_preflight(
    request: Task5afRequest,
    row: &RowName,
    run_id: String,
) -> Result<Task5afOutcome> {
    let row_dir = request.evidence_dir.join("rows").join(row.as_str());
    reject_symlinked_evidence_component(
        &request.evidence_dir,
        &Path::new("rows").join(row.as_str()),
    )?;
    fs::create_dir_all(&row_dir).with_context(|| {
        format!(
            "failed to create Task 5af row evidence directory {}",
            row_dir.display()
        )
    })?;

    let stdout_rel = format!("rows/{}/stdout.txt", row.as_str());
    let stderr_rel = format!("rows/{}/stderr.txt", row.as_str());
    write_new_file(
        &request.evidence_dir.join(&stdout_rel),
        format!("Task 5af row {} safe preflight only\n", row.as_str()).as_bytes(),
    )?;
    write_new_file(&request.evidence_dir.join(&stderr_rel), b"")?;

    let ledger = ledger_jsonl(&run_id, row)?;
    write_new_file(
        &request.evidence_dir.join("ledger.jsonl"),
        ledger.as_bytes(),
    )?;

    let manifest = Manifest {
        schema: SCHEMA,
        task: TASK,
        run_id: run_id.clone(),
        row_count: 1,
        started_row_count: 1,
        finished_row_count: 1,
        live_hvf_started: false,
        qemu_started: false,
        claims_allowed: Vec::new(),
        row: row.as_str().to_string(),
        command: request.command,
        env: request.env.into_iter().collect(),
        paths: EvidencePaths {
            stdout: stdout_rel,
            stderr: stderr_rel,
        },
        classification: SAFE_PREFLIGHT_CLASSIFICATION,
    };
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).context("failed to serialize Task 5af manifest")?;
    write_new_file(&request.evidence_dir.join("manifest.json"), &manifest_json)?;

    Ok(Task5afOutcome {
        run_id,
        classification: SAFE_PREFLIGHT_CLASSIFICATION.to_string(),
    })
}

fn reject_reused_ledger(
    request: &Task5afRequest,
    row: &RowName,
    run_id: &str,
    reuse_ledger: &Path,
) -> Result<Task5afOutcome> {
    let sha256_before = sha256_file(reuse_ledger)
        .with_context(|| format!("failed to hash --reuse-ledger {}", reuse_ledger.display()))?;
    reject_symlinked_evidence_component(&request.evidence_dir, Path::new(""))?;
    fs::create_dir_all(&request.evidence_dir).with_context(|| {
        format!(
            "failed to create Task 5af rejection evidence directory {}",
            request.evidence_dir.display()
        )
    })?;
    let sha256_after = sha256_file(reuse_ledger).with_context(|| {
        format!(
            "failed to re-hash --reuse-ledger {}",
            reuse_ledger.display()
        )
    })?;
    let receipt = ReuseLedgerRejection {
        schema: SCHEMA,
        task: TASK,
        run_id: run_id.to_string(),
        row: row.as_str().to_string(),
        classification: REUSE_LEDGER_REJECTION,
        reuse_ledger_path: reuse_ledger.display().to_string(),
        sha256_before,
        sha256_after,
        live_hvf_started: false,
        qemu_started: false,
    };
    let receipt_json = serde_json::to_vec_pretty(&receipt)
        .context("failed to serialize Task 5af reuse-ledger rejection receipt")?;
    write_new_file(
        &request.evidence_dir.join("reuse-ledger-rejection.json"),
        &receipt_json,
    )?;
    bail!("--reuse-ledger is not supported in this first Task 5af slice; rejection receipt written")
}

fn ledger_jsonl(run_id: &str, row: &RowName) -> Result<String> {
    let mut ledger = String::new();
    for event in ["row_planned", "row_started", "row_finished"] {
        let entry = LedgerEvent {
            schema: SCHEMA,
            task: TASK,
            run_id,
            row: row.as_str(),
            event,
            classification: SAFE_PREFLIGHT_CLASSIFICATION,
            live_hvf_started: false,
            qemu_started: false,
        };
        ledger.push_str(
            &serde_json::to_string(&entry).context("failed to serialize Task 5af ledger event")?,
        );
        ledger.push('\n');
    }
    Ok(ledger)
}

fn fresh_run_id() -> Result<String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before unix epoch")?;
    let sequence = NEXT_RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!(
        "task5af-{}-{}-{}-{sequence}",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        std::process::id()
    ))
}
