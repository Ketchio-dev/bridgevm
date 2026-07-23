//! One-shot connect-and-execute helpers.

use crate::*;
use serde_json::Value;
use std::path::Path;

pub fn query_status(socket_path: &Path) -> Result<QmpStatus, QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let value = client.execute(QmpCommand::query_status())?;
    Ok(QmpStatus {
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        running: value
            .get("running")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub fn quit(socket_path: &Path) -> Result<(), QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::quit())?;
    Ok(())
}

pub fn stop(socket_path: &Path) -> Result<(), QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::stop())?;
    Ok(())
}

pub fn cont(socket_path: &Path) -> Result<(), QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::cont())?;
    Ok(())
}
