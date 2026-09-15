//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    if matches!(&cli.command, Command::App(_)) && (cli.store.is_some() || cli.socket.is_some()) {
        <Cli as clap::CommandFactory>::command().error(
            clap::error::ErrorKind::ArgumentConflict,
            "app commands use the native library; omit --store and --socket, and use --library if needed",
        ).exit();
    }
    if let Command::App(args) = cli.command {
        return app_cli::run(args);
    }
    if let Some(socket) = cli.socket {
        return run_via_daemon(&socket, cli.command);
    }

    let store = cli.store.map(VmStore::new).unwrap_or_else(VmStore::default);

    local_dispatch::run(store, cli.command)
}

pub(crate) fn run_via_daemon(socket: &Path, command: Command) -> Result<()> {
    let request = request_for(command)?;
    let response = send_request(socket, request)?;
    print_daemon_response(response)
}
