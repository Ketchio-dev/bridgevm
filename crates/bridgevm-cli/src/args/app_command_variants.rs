use super::*;

#[derive(Debug, Subcommand)]
pub(crate) enum AppCommand {
    #[command(flatten)]
    Query(AppQueryCommand),
    /// Start a saved own-HVF VM and wait for confirmed initial helper startup.
    Start { id: String },
    /// Stop an app-owned runtime and wait for confirmed owned-process cleanup.
    Stop { id: String },
}
