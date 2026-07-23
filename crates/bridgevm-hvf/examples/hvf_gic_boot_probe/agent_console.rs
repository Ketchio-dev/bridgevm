use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;

use super::{hv_vcpus_exit, HvVcpuT, EXIT_CANCELED};

#[path = "host_pasteboard.rs"]
mod host_pasteboard;
#[path = "share_sync.rs"]
mod share_sync;

use host_pasteboard::HostPasteboard;
use share_sync::{GuestFileOutcome, HostFile, LsEntry, ShareSync, SkipReason, SyncAction};

#[cfg(test)]
#[path = "agent_console_tests.rs"]
mod agent_console_tests;
#[path = "agent_console/clipboard.rs"]
mod clipboard;
#[path = "agent_console/config.rs"]
mod config;
#[path = "agent_console/control_file.rs"]
mod control_file;
#[path = "agent_console/harness_protocol.rs"]
mod harness_protocol;
#[path = "agent_console/protocol.rs"]
mod protocol;
#[path = "agent_console/resident_service.rs"]
mod resident_service;
#[path = "agent_console/service_wake.rs"]
mod service_wake;
#[path = "agent_console/share.rs"]
mod share;
#[path = "agent_console/state.rs"]
mod state;
use clipboard::*;
use config::*;
use control_file::*;
use protocol::*;
pub use service_wake::ServiceWake;
use share::*;
pub use state::AgentConsoleHarness;
use state::*;
