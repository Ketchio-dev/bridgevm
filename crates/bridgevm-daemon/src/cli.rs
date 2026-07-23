//! The bridgevmd argument surface and the startup path main() calls.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_storage::VmStore;
use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "bridgevmd", about = "BridgeVM core daemon scaffold")]
pub(crate) struct Args {
    #[arg(long, value_name = "PATH")]
    pub(crate) store: Option<PathBuf>,
    #[arg(long, default_value = "bridgevmd.sock", value_name = "SOCKET")]
    pub(crate) socket_name: String,
    #[arg(long)]
    pub(crate) once: bool,
    #[arg(long, default_value_t = 250, value_name = "MILLIS")]
    pub(crate) reconcile_interval_ms: u64,
}

pub(crate) fn run() -> Result<()> {
    let args = Args::parse();
    let store = args
        .store
        .map(VmStore::new)
        .unwrap_or_else(VmStore::default);
    store
        .ensure()
        .context("failed to initialize BridgeVM store")?;

    let socket_path = store.root().join("run").join(args.socket_name);
    println!("bridgevmd store: {}", store.root().display());
    println!("bridgevmd socket: {}", socket_path.display());
    println!("bridgevmd status: metadata service ready");

    if args.once {
        return Ok(());
    }

    serve(
        store,
        &socket_path,
        Duration::from_millis(args.reconcile_interval_ms),
    )
}
