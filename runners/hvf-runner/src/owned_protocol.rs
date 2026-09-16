//! Strict v1 DTOs. Secret HELLO deliberately has no Debug implementation.
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Hello {
    pub schema_version: u8,
    pub kind: String,
    pub run_token: String,
    pub sequence: u64,
    #[serde(rename = "manifestSHA256")]
    pub manifest_sha256: String,
    pub key_hex: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Stop {
    pub schema_version: u8,
    pub kind: String,
    pub run_token: String,
    pub sequence: u64,
    #[serde(rename = "operationID")]
    pub operation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Event {
    pub schema_version: u8,
    pub kind: String,
    pub run_token: String,
    #[serde(rename = "runnerPID")]
    pub runner_pid: u32,
    pub sequence: u64,
    pub ready: Option<Ready>,
    pub stop_ack: Option<Ack>,
    pub child: Option<Child>,
    pub unconfirmed: Option<Unconfirmed>,
    pub complete: Option<Complete>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Ready {
    #[serde(rename = "manifestSHA256")]
    pub manifest_sha256: String,
    pub generation: u64,
    pub term_millis: u32,
    pub kill_reap_millis: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Ack {
    #[serde(rename = "operationID")]
    pub operation_id: String,
    pub disposition: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Child {
    pub role: String,
    pub pid: u32,
    pub generation: Option<u64>,
    pub reason: Option<String>,
    pub status: Option<i32>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Unconfirmed {
    pub role: String,
    pub pid: u32,
    pub generation: Option<u64>,
    pub reason: String,
    pub media_lease_disposition: String,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Summary {
    pub spawned_count: u64,
    pub reaped_count: u64,
    pub last: Option<Child>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Complete {
    #[serde(rename = "operationID")]
    pub operation_id: Option<String>,
    pub cause: String,
    pub outcome: String,
    pub failure_code: Option<String>,
    pub helper: Summary,
    pub swtpm: Summary,
    pub media_lease_disposition: String,
    pub runtime_directory_disposition: String,
}
impl Event {
    pub fn new(kind: &str) -> Self {
        Self {
            schema_version: 1,
            kind: kind.into(),
            run_token: String::new(),
            runner_pid: 0,
            sequence: 0,
            ready: None,
            stop_ack: None,
            child: None,
            unconfirmed: None,
            complete: None,
        }
    }
}
