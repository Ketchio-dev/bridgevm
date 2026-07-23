//! QemuError and the idle/unavailable classification of QMP I/O failures.

use bridgevm_config::VmMode;
use bridgevm_network::NetworkPlanError;
use std::io::ErrorKind;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QemuError {
    #[error("QEMU command builder only supports Compatibility Mode manifests, got {0}")]
    UnsupportedMode(VmMode),
    #[error("QEMU launch does not support {0} networking yet")]
    UnsupportedNetworkMode(String),
    #[error(
        "QEMU launch blocker {blocker}: {mode} networking requires an advanced Compatibility Mode QEMU schema before args can be generated; requirement: {requirement}"
    )]
    UnsupportedNetworkRequirement {
        mode: String,
        blocker: String,
        requirement: String,
    },
    #[error("QMP I/O error: {0}")]
    QmpIo(#[from] std::io::Error),
    #[error("QMP JSON error: {0}")]
    QmpJson(#[from] serde_json::Error),
    #[error("QMP response did not include a return value: {0}")]
    QmpProtocol(String),
    #[error("QEMU network plan rejected: {0}")]
    NetworkPlan(#[from] NetworkPlanError),
    #[error("windows-installer boot mode requires boot.installerImage")]
    MissingInstallerImage,
}

pub fn is_qmp_status_unavailable(error: &QemuError) -> bool {
    matches!(
        error,
        QemuError::QmpIo(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset
                    | ErrorKind::WouldBlock
                    | ErrorKind::TimedOut
                    | ErrorKind::UnexpectedEof
            )
    )
}

impl QemuError {
    pub fn is_qmp_idle(&self) -> bool {
        matches!(
            self,
            QemuError::QmpIo(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::UnexpectedEof
                )
        )
    }
}
