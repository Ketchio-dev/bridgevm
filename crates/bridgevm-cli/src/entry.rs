//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(socket) = cli.socket {
        return run_via_daemon(&socket, cli.command);
    }

    let store = cli.store.map(VmStore::new).unwrap_or_else(VmStore::default);

    match cli.command {
        Command::List => list(&store),
        Command::Templates => templates(),
        Command::Create(args) => create(&store, args),
        Command::Status(args) => status(&store, args),
        Command::Start(args) => transition(
            &store,
            args,
            VmRuntimeState::Running,
            "Metadata state recorded for",
        ),
        Command::Stop(args) => stop_backend_local(&store, args),
        Command::Restart(args) => restart_local(&store, args),
        Command::Suspend(args) => suspend_backend_local(&store, args),
        Command::Resume(args) => resume_backend_local(&store, args),
        Command::Display(args) => display_backend_local(&store, args),
        Command::Delete(args) => delete(&store, args),
        Command::Export(args) => export_vm(&store, args),
        Command::Import(args) => import_vm(&store, args),
        Command::Clone(args) => clone_vm(&store, args),
        Command::Diagnostics(args) => diagnostics(&store, args),
        Command::Logs(args) => logs(&store, args),
        Command::Performance(args) => performance(&store, args),
        Command::Metadata(args) => metadata(&store, args),
        Command::Snapshot(args) => snapshot(&store, args),
        Command::Disk(args) => disk(&store, args),
        Command::Port(args) => port(&store, args),
        Command::NetworkPlan(args) => network_plan(&store, args),
        Command::Share(args) => share(&store, args),
        Command::Media(args) => media(&store, args),
        Command::GuestTools(args) => guest_tools(&store, args),
        Command::Resources(args) => resources(&store, args),
        Command::RuntimeControl(args) => runtime_control(&store, args),
        Command::QemuArgs(args) => qemu_args(&store, args),
        Command::PrepareRun(args) => prepare_run(&store, args),
        Command::BootMedia(args) => boot_media(&store, args),
        Command::Ssh(args) => ssh(&store, args),
        Command::Open(args) => open_port(&store, args),
        Command::Run(args) => run_backend_local(&store, args),
        Command::Readiness(args) => readiness(&store, args),
        Command::LifecyclePlan(args) => lifecycle_plan(&store, args),
        Command::QmpSocket(args) => qmp_socket(&store, args),
        Command::QmpStatus(args) => qmp_status(&store, args),
        Command::QmpStop(args) => qmp_control(&store, args, "stop", qmp_stop),
        Command::QmpCont(args) => qmp_control(&store, args, "cont", qmp_cont),
        Command::RunnerStatus(args) => runner_status(&store, args),
        Command::Recommend(args) => recommend(args),
        Command::Hvf(args) => hvf(args),
        Command::Store(StoreCommand::Doctor) => doctor(&store),
        Command::Doctor => doctor(&store),
    }
}

pub(crate) fn run_via_daemon(socket: &Path, command: Command) -> Result<()> {
    let request = request_for(command)?;
    let response = send_request(socket, request)?;
    print_daemon_response(response)
}
