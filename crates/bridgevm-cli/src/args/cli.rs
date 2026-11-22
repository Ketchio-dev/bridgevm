//! Top-level command help and global transport options.

use crate::*;

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm",
    about = "BridgeVM CLI for native app inventory, legacy stores and local HVF tools",
    after_help = "Legacy VM commands use manifest.yaml bundles. Native macOS app registrations use vm.json; these formats are not interchangeable.

Native app inventory (no app window or guest launch):
  bridgevm app list --json
  bridgevm app inspect VM_ID

Own-HVF reports (no guest launch):
  bridgevm hvf virtio-gpu-trace-report --trace PATH
  bridgevm hvf title-gate-report --help

Legacy store inspection:
  bridgevm --store PATH list

Use COMMAND --help for details."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
    /// Local legacy VmStore root (manifest.yaml bundles); ignored with --socket.
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) store: Option<PathBuf>,
    /// Use bridgevmd for supported legacy commands; not supported by app or hvf.
    #[arg(long, global = true, value_name = "SOCKET")]
    pub(crate) socket: Option<PathBuf>,
}
