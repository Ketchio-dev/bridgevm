extension NativeCLI {
    static let help = """
    BridgeVM native app CLI (bridgevm-native-cli-v1)

    Usage: BridgeVMControl --cli <command> [--library ABSOLUTE_PATH] [--json]
      list          List saved VMs in the native app library.
      inspect ID    Inspect one exact VM ID from list (including IDs with non-English characters).
      readiness ID  Check saved own-HVF VM launch prerequisites without starting it.

    Examples:
      BridgeVMControl --cli list --json
      BridgeVMControl --cli inspect dev-vm
      BridgeVMControl --cli readiness dev-vm --json

    Uses the same vm.json configuration as the native app. Reads never repair,
    migrate, create or start a VM. Inventory runtime is unobserved. Recovery records
    are reported without interpreting or modifying them. CPU/memory are saved
    values, not measured usage. Inventory JSON schema: bridgevm.app-library.v1.
    Inventory exit codes: 0 complete, 1 unavailable/incomplete, 2 invalid usage.

    Readiness uses the app's existing own-HVF prerequisite checks. It does not
    open Keychain, boot a VM, or prove guest behavior or release readiness.
    Launch blockers, product release gates and limitations are separate.
    Unexamined recovery records are reported without changing them.
    Readiness JSON schema: bridgevm.app-readiness.v1.
    Readiness exit codes: 0 launch prerequisites ready, 1 blocked/unavailable,
    2 invalid usage. Unsupported engines and pending installation are blocked.
    The Rust bridgevm CLI offers the same library as `bridgevm app ...` and
    HVF trace and log reports as `bridgevm hvf ...`.
    """ + "\n\n" + createWindowsHelp + "\n\n" + statusHelp + "\n\n" + installHelp + "\n\n" + snapshotHelp + "\n\n" + stopHelp + "\n\n" + startHelp
}

extension NativeCLI {
    static let createWindowsHelp = """
    create-windows NAME --iso ABSOLUTE_PATH
      Create a saved installation-pending own-HVF Windows VM. The ISO is copied
      into its managed bundle before vm.json is published. This command does not
      install Windows, boot a guest, open a window, or accept passwords, recovery
      keys, unattended files, guest payload paths, driver overrides, or
      experimental graphics.

      Optional product settings:
        --disk-gib 64|96|128|256|512
        --memory-mib 2048|4096|6144|8192|12288|16384|24576|32768
        --cpus COUNT
        --resolution 1280x800|1440x900|1920x1080|2560x1440
        --no-network
        --fetch-drivers

      Defaults: 64 GiB, 6144 MiB (4096 MiB on an 8 GiB Mac), 4 CPUs, 1440x900,
      shared NAT network.
      \(NativeCLICreateWindowsGuidance.help)
      JSON schema: bridgevm.app-create-windows.v2.
      Exit codes: 0 registration confirmed, 1 driver preparation, creation or
      confirmation failed, 2 invalid name, option, path, media, resource value
      or host CPU request.
    """
}

extension NativeCLI {
    static let statusHelp = """
    Runtime observation:
      status ID     Query the running native app's retained own-HVF session.
      BridgeVMControl --cli status dev-vm --json

    Status includes retained sessions even after a saved registration is removed.
    It distinguishes owned children, attached observations and observed child exits.
    An unobserved runtime is not assumed stopped. Saved configuration comparisons
    are unknown when the configuration is missing or unreadable. Status does not
    launch the app or a VM, read Keychain, or infer guest health from a process.
    Status JSON schema: bridgevm.app-runtime.v1; scope: app-observation.
    Status exit codes: 0 complete app observation (including no retained session),
    1 app observation unavailable, 2 invalid usage.
    """
}

extension NativeCLI {
    static let installHelp = """
    Install commands (already-running compatible app required):
      install ID         Start or reuse the saved pending Windows installation.
      install-status ID  Read the retained installation phase and recent bounded log.
      install-cancel ID  Request cancellation when the retained phase permits it.

    These commands use metadata/hvf-install.json created with the VM. They do not
    accept an ISO path, password, recovery key, or unattended secret on argv. The
    app retains installation work after the CLI disconnects. A successful install
    request means the app accepted or found the operation; it does not mean Windows
    finished installing or booted. Use install-status to observe later progress.
    Exit codes: 0 request/observation available, 1 refused/unavailable, 2 usage.
    JSON schema: bridgevm.app-install-command.v2.
    """
}

extension NativeCLI {
    static let snapshotHelp = """
    Powered-off snapshot commands:
      snapshot-create ID   Save the current disk and UEFI vars as one verified pair.
      snapshot-restore ID  Restore the newest snapshot for the powered-off VM.
      snapshot-export ID OUTPUT  Export the current verified raw pair to OUTPUT.
    The saved VM must use the own-HVF backend and must not be installation-pending
    or running. OUTPUT is an absolute directory outside the VM bundle. Media and
    helper paths come from the saved app library; disk, vars and secrets stay off argv.
    JSON schema: bridgevm.app-snapshot.v1.
    Ctrl-C or SIGTERM stops the helper and waits for it: disk, vars and OUTPUT
    are left as before or complete, never mixed.
    Exit codes: 0 complete, 1 refused/failed, 130/143 cancelled, 2 invalid usage.
    """
}

extension NativeCLI {
    static let stopHelp = """
    Owned runtime stop:
      stop ID       Ask the running app to stop its exact retained owned run.
      BridgeVMControl --cli stop dev-vm --json

    Waits up to 210 seconds for supervisor cleanup and retained runner exit.
    Requests reuse the same target and operation; retries do not reset the stop
    deadline. Attached observations and process-name matches cannot be stopped.
    The app must already be running. No app or VM is launched by this command.
    A timeout ends this CLI observation; admitted cleanup remains app-owned.
    Confirmed runtime cleanup does not prove normal guest shutdown.
    Stop JSON schema: bridgevm.app-stop.v2; scope: app-owned-runtime-control.
    Stop exit codes: 0 cleanup and runner exit confirmed, 1 refused/unconfirmed,
    2 invalid usage. Pending admission alone never returns a successful exit.
    """
}

extension NativeCLI {
    static let startHelp = """
    Owned runtime start:
      start ID      Start one saved, installed own-HVF VM in the running app.
      BridgeVMControl --cli start dev-vm --json

    Observes startup for up to 30 seconds. Success requires the supervisor READY
    message and initial helper startup; it does not prove guest boot or display.
    The app and its library model must already be running. No app/window opens.
    The vTPM key is handled as for Start in the app: a new VM gets its key then.
    Exact retries reuse one operation and saved configuration. A deadline or CLI
    disconnect does not release pending app-owned work or start a replacement.
    Start JSON schema: bridgevm.app-start.v2; scope: app-owned-runtime-start.
    Start exit codes: 0 startup confirmed, 1 refused/unconfirmed, 2 invalid usage.
    """
}
