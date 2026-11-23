//! Top-level command help.

use crate::*;

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm",
    about = "BridgeVM CLI for the native app library and local HVF tools",
    after_help = "Native app inventory (no app window or guest launch):
  bridgevm app list --json
  bridgevm app inspect VM_ID

Own-HVF reports (no guest launch):
  bridgevm hvf virtio-gpu-trace-report --trace PATH
  bridgevm hvf title-gate-report --help

Use COMMAND --help for details."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}
