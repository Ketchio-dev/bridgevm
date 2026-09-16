//! Unchanged swtpm argv and stdio policy, separate from owned lifecycle.
use super::VtpmConfig;
use std::{
    path::Path,
    process::{Command, Stdio},
};
pub(super) fn build(config: &VtpmConfig, data_socket: &Path, control_socket: &Path) -> Command {
    let mut command = Command::new(&config.swtpm_bin);
    command
        .args(["socket", "--tpm2", "--tpmstate"])
        .arg(format!("dir={}", config.state_dir.display()))
        .arg("--server")
        .arg(format!(
            "type=unixio,path={},mode=0600",
            data_socket.display()
        ))
        .arg("--ctrl")
        .arg(format!(
            "type=unixio,path={},mode=0600",
            control_socket.display()
        ))
        .args(["--flags", "not-need-init,startup-clear"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if config.state_key.is_some() {
        command
            .args(["--key", "fd=0,format=binary,mode=aes-256-cbc"])
            .stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    command
}
