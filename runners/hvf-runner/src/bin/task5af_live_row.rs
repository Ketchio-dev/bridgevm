use anyhow::Result;
use clap::Parser;
use hvf_runner::task5af::run_task5af;
use hvf_runner::task5af::Task5afRequest;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "task5af_live_row",
    about = "BridgeVM HVF Task 5af live-row evidence runner"
)]
struct Cli {
    #[arg(long, value_name = "PATH")]
    evidence_dir: PathBuf,
    #[arg(long, default_value = "baseline")]
    row: String,
    #[arg(long, value_name = "PATH")]
    reuse_ledger: Option<PathBuf>,
    #[arg(long)]
    allow_live: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let live_env_allowed = env::var("BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE").ok().as_deref() == Some("1");
    let env_snapshot = vec![(
        "BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE".to_string(),
        env::var("BRIDGEVM_HVF_ALLOW_TASK5AF_LIVE").unwrap_or_else(|_| "<unset>".to_string()),
    )];
    let _outcome = run_task5af(Task5afRequest {
        evidence_dir: cli.evidence_dir,
        row: cli.row,
        reuse_ledger: cli.reuse_ledger,
        allow_live: cli.allow_live,
        live_env_allowed,
        command: env::args().collect(),
        env: env_snapshot,
    })?;
    Ok(())
}
