include!("app_create_windows_args.rs");
#[rustfmt::skip]
#[derive(Debug, Subcommand)]
pub(crate) enum AppCommand {
    #[command(flatten)]
    Query(AppQueryCommand),
    #[command(about = "Create a saved installation-pending own-HVF Windows VM; does not install or boot it.")] CreateWindows(AppCreateWindowsArgs),
    #[command(about = "Start a saved own-HVF VM and wait for confirmed initial helper startup.")] Start { id: String },
    #[command(about = "Stop an app-owned runtime and wait for confirmed owned-process cleanup.")] Stop { id: String },
    #[command(about = "Start or reuse the pending request already saved with this VM; ISO paths and secrets are not accepted on argv.")] Install { id: String }, #[command(about = "Read retained progress for the saved pending installation; secrets are not accepted on argv.")] InstallStatus { id: String }, #[command(about = "Request cancellation of the retained installation; secrets are not accepted on argv.")] InstallCancel { id: String },
}
