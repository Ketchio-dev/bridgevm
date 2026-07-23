//! QmpCommand and its constructors for every QMP verb used.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QmpCommand {
    pub execute: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

impl QmpCommand {
    pub fn capabilities() -> Self {
        Self {
            execute: "qmp_capabilities".to_string(),
            arguments: None,
        }
    }

    pub fn query_status() -> Self {
        Self {
            execute: "query-status".to_string(),
            arguments: None,
        }
    }

    pub fn stop() -> Self {
        Self {
            execute: "stop".to_string(),
            arguments: None,
        }
    }

    pub fn cont() -> Self {
        Self {
            execute: "cont".to_string(),
            arguments: None,
        }
    }

    pub fn quit() -> Self {
        Self {
            execute: "quit".to_string(),
            arguments: None,
        }
    }

    /// Build the job-based `snapshot-save` command that writes a full internal
    /// VM snapshot (CPU + RAM + device state) into the qcow2 disk under `tag`.
    ///
    /// `job_id` lets the caller poll completion via `query-jobs`. `devices`
    /// lists the block node names whose qcow2 receives the snapshot;
    /// `vmstate` names the device that stores the machine state (RAM/CPU).
    pub fn snapshot_save(job_id: &str, tag: &str, vmstate: &str, devices: &[String]) -> Self {
        Self {
            execute: "snapshot-save".to_string(),
            arguments: Some(serde_json::json!({
                "job-id": job_id,
                "tag": tag,
                "vmstate": vmstate,
                "devices": devices,
            })),
        }
    }

    /// Build the job-based `snapshot-load` command that restores a full
    /// internal VM snapshot previously written by [`QmpCommand::snapshot_save`].
    pub fn snapshot_load(job_id: &str, tag: &str, vmstate: &str, devices: &[String]) -> Self {
        Self {
            execute: "snapshot-load".to_string(),
            arguments: Some(serde_json::json!({
                "job-id": job_id,
                "tag": tag,
                "vmstate": vmstate,
                "devices": devices,
            })),
        }
    }

    /// Build the `query-jobs` command used to poll job-based commands such as
    /// `snapshot-save`/`snapshot-load` to completion.
    pub fn query_jobs() -> Self {
        Self {
            execute: "query-jobs".to_string(),
            arguments: None,
        }
    }
}
