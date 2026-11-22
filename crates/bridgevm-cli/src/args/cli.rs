//! Top-level command help.

use crate::*;

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm",
    about = "BridgeVM CLI for the native app library and local HVF tools",
    after_help = "Native app inventory (no app window or guest launch):
  bridgevm app list --json
  bridgevm app inspect VM_ID

Own-HVF queries (no guest launch):
  bridgevm hvf host-capabilities
  bridgevm hvf windows-plan
  bridgevm hvf machine-plan --memory-gib 6 --vcpus 4

Use COMMAND --help for effects and explicit probe opt-ins."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}
