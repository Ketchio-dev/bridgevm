//! QMP connect and negotiate, event drain reporting, terminal-status detection, metadata write-back.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_qemu::query_status;
use bridgevm_qemu::QemuError;
use bridgevm_qemu::QmpClient;
use bridgevm_qemu::QmpEventDrain;
use bridgevm_storage::QmpSupervisorMetadata;
use bridgevm_storage::VmStore;
use std::path::Path;
use std::time::Duration;

pub(crate) fn connect_supervisor_qmp(socket_path: &Path) -> Result<QmpClient, QemuError> {
    let mut client = QmpClient::connect_with_timeout(socket_path, Duration::from_millis(25))?;
    client.negotiate()?;
    Ok(client)
}

pub(crate) fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

pub(crate) struct QmpSupervisorReport {
    pub(crate) terminal: bool,
    pub(crate) drain: Option<QmpEventDrain>,
}

pub(crate) fn qmp_supervisor_report(
    client: &mut Option<QmpClient>,
    socket_path: &Path,
) -> QmpSupervisorReport {
    let Some(client_ref) = client.as_mut() else {
        return QmpSupervisorReport {
            terminal: qmp_status_is_terminal(socket_path),
            drain: None,
        };
    };

    match client_ref.drain_events(QMP_SUPERVISOR_DRAIN_LIMIT) {
        Ok(drain) => {
            let terminal = drain.has_terminal_event();
            let should_record =
                drain.envelopes_read > 0 || drain.limit_reached || drain.terminal_event.is_some();
            QmpSupervisorReport {
                terminal,
                drain: should_record.then_some(drain),
            }
        }
        Err(error) if error.is_qmp_idle() => QmpSupervisorReport {
            terminal: false,
            drain: None,
        },
        Err(_) => {
            *client = None;
            QmpSupervisorReport {
                terminal: qmp_status_is_terminal(socket_path),
                drain: None,
            }
        }
    }
}

pub(crate) fn qmp_status_is_terminal(socket_path: &Path) -> bool {
    query_status(socket_path)
        .map(|status| status.is_terminal())
        .unwrap_or(false)
}

pub(crate) fn write_qmp_supervisor_metadata(
    store: &VmStore,
    name: &str,
    drain: &QmpEventDrain,
) -> Result<()> {
    store
        .write_qmp_supervisor_metadata(
            name,
            &QmpSupervisorMetadata {
                events: drain.events.clone(),
                terminal_event: drain.terminal_event.clone(),
                envelopes_read: drain.envelopes_read,
                limit_reached: drain.limit_reached,
                updated_at_unix: now_unix(),
            },
        )
        .context("failed to write QMP supervisor metadata")
}

pub(crate) const QMP_SUPERVISOR_DRAIN_LIMIT: usize = 16;
