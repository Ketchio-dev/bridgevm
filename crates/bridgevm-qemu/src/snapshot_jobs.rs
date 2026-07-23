//! Job-based snapshot suspend and query-jobs polling to a terminal status.

use crate::*;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Internal snapshot tag used for Compatibility Mode suspend/resume.
pub const COMPAT_SUSPEND_SNAPSHOT_TAG: &str = "bridgevm-suspend";

/// Terminal states for a QEMU job (`query-jobs[].status`).
pub(crate) fn job_status_is_terminal(status: &str) -> bool {
    matches!(status, "concluded" | "aborting" | "null")
}

/// Poll `query-jobs` until `job_id` reaches a terminal status or `timeout`
/// elapses. Returns the job's `error` field if it concluded with one.
pub(crate) fn wait_for_job(
    client: &mut QmpClient,
    job_id: &str,
    timeout: Duration,
) -> Result<(), QemuError> {
    let deadline = std::time::Instant::now() + timeout;
    let mut observed = false;
    loop {
        let jobs = client.execute(QmpCommand::query_jobs())?;
        let job = jobs
            .as_array()
            .and_then(|jobs| {
                jobs.iter()
                    .find(|job| job.get("id").and_then(Value::as_str) == Some(job_id))
            })
            .cloned();

        match job {
            Some(job) => {
                observed = true;
                let status = job
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if job_status_is_terminal(status) {
                    if let Some(error) = job.get("error").and_then(Value::as_str) {
                        return Err(QemuError::QmpProtocol(format!(
                            "snapshot job '{job_id}' failed: {error}"
                        )));
                    }
                    return Ok(());
                }
            }
            // QEMU drops concluded jobs from `query-jobs` after dismissal. Only
            // treat a vanished job as complete once we've actually OBSERVED it
            // running -- otherwise a snapshot-save that failed/was reaped before
            // we ever saw it would be silently reported as a successful snapshot
            // (and resume would later -loadvm a snapshot that does not exist).
            None if observed => return Ok(()),
            None => {}
        }

        if std::time::Instant::now() >= deadline {
            return Err(QemuError::QmpProtocol(if observed {
                format!("timed out waiting for snapshot job '{job_id}'")
            } else {
                format!("snapshot job '{job_id}' was never observed; snapshot-save likely failed")
            }));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Pause the guest and save a full internal VM snapshot into the primary
/// qcow2, then leave QEMU paused. Used by Compatibility Mode suspend.
///
/// Sequence: negotiate -> `stop` (pause CPUs) -> `snapshot-save` (job) ->
/// wait for the job to conclude. The caller is responsible for `quit`ing QEMU
/// afterwards.
pub fn suspend_to_snapshot(socket_path: &Path, timeout: Duration) -> Result<(), QemuError> {
    let mut client = QmpClient::connect_with_timeout(socket_path, Duration::from_secs(2))?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::stop())?;
    let devices = vec![COMPAT_PRIMARY_BLOCK_NODE.to_string()];
    let _ = client.execute(QmpCommand::snapshot_save(
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_PRIMARY_BLOCK_NODE,
        &devices,
    ))?;
    wait_for_job(&mut client, COMPAT_SUSPEND_SNAPSHOT_TAG, timeout)?;
    Ok(())
}
