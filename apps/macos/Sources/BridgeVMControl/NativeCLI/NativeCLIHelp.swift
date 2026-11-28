extension NativeCLI {
    static let help = """
    BridgeVM native app CLI (bridgevm-native-cli-v1)

    Usage: bridgevm app <command> [--library ABSOLUTE_PATH] [--json]
       or: BridgeVMControl --cli <command> [--library ABSOLUTE_PATH] [--json]
      list          List saved VMs in the native app library.
      inspect ID    Inspect one exact VM ID from list (including IDs with non-English characters).
      readiness ID  Check whether a saved Windows VM is ready to start, without starting it.

    Examples:
      bridgevm app list --json
      bridgevm app inspect dev-vm
      bridgevm app readiness dev-vm --json

    Uses the same vm.json configuration as the native app. Reads never repair,
    migrate, create or start a VM. List and inspect don't check whether a VM is
    running; use status. Recovery records are reported without interpreting or
    modifying them. CPU/memory are saved values, not measured usage. Inventory
    JSON schema: bridgevm.app-library.v1.
    Inventory exit codes: 0 complete, 1 unavailable/incomplete, 2 invalid usage.

    Readiness runs the app's start checks without opening Keychain or starting
    Windows. Unexamined recovery records are reported without changing them.
    Readiness JSON schema: bridgevm.app-readiness.v1.
    Readiness exit codes: 0 launch prerequisites ready, 1 blocked/unavailable,
    2 invalid usage. Unsupported engines and pending installation are blocked.
    The bridgevm command also offers HVF trace and log reports as `bridgevm hvf ...`.
    """ + "\n\n" + createWindowsHelp + "\n\n" + statusHelp + "\n\n" + installHelp + "\n\n" + snapshotHelp + "\n\n" + stopHelp + "\n\n" + startHelp
}

extension NativeCLI {
    static let createWindowsHelp = """
    create-windows NAME --iso ABSOLUTE_PATH
      Create a saved Windows VM that is waiting to be installed. The ISO is
      copied into the VM's folder before vm.json is published. This command
      does not install Windows, boot a guest, open a window, or accept
      passwords, recovery keys, unattended files, guest payload paths, driver
      overrides, or experimental graphics.

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
      Exit codes: 0 VM created and saved, 1 driver preparation, creation or
      confirmation failed, 2 invalid name, option, path, media, resource value
      or host CPU request.
    """
}

extension NativeCLI {
    static let statusHelp = """
    VM status:
      status ID     Ask the running app whether it is running this Windows VM.
      bridgevm app status dev-vm --json

    Status also reports a VM the app still tracks after it was removed from the
    library. It tells apart a VM that BridgeVM started, a VM that another app or
    tool started, and a VM that BridgeVM started and that has since stopped. If the
    app reports nothing, that does not prove the VM is stopped. If vm.json is
    missing or unreadable, status cannot tell whether the VM uses its saved settings.
    Status does not open the app, start a VM, read Keychain, or check whether
    Windows is healthy.
    Status JSON schema: bridgevm.app-runtime.v1; scope: app-observation.
    Status exit codes: 0 the app answered (even if it is not running this VM),
    1 the app did not answer, 2 invalid usage.
    """
}

extension NativeCLI {
    static let installHelp = """
    Install commands (BridgeVM must already be open):
      install ID         Start the Windows installation saved with the VM, or join
                         the one already running.
      install-status ID  Show the installation phase and recent log lines.
      install-cancel ID  Cancel the installation while it can still be canceled.

    These commands use metadata/hvf-install.json created with the VM. They do not
    accept an ISO path, password, recovery key, or unattended secret on argv. The
    installation keeps running in the app if the CLI exits. A successful install
    request means the app started the installation or found one already running;
    it does not mean Windows finished installing or booted. Use install-status to
    follow progress.
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
    The VM must be an installed BridgeVM Windows VM that is shut down. OUTPUT is
    an absolute directory outside the VM's folder. Disk and vars paths come
    from the saved app library, not argv; secrets stay off argv too.
    JSON schema: bridgevm.app-snapshot.v1.
    Ctrl-C stops the snapshot; disk, vars and OUTPUT stay unchanged or complete.
    Exit codes: 0 complete, 1 refused/failed, 130/143 canceled, 2 invalid usage.
    """
}

extension NativeCLI {
    static let stopHelp = """
    Stop a VM:
      stop ID       Ask the running app to shut down the VM that BridgeVM started.
      bridgevm app stop dev-vm --json

    Waits up to 210 seconds for the VM to shut down and close. If an answer is
    lost, the command asks about the same stop again without restarting the wait.
    It stops only a VM that BridgeVM started, never one that another app or tool
    started or a process found by name. BridgeVM must already be open; this
    command does not open the app or start a VM. A timeout or Ctrl-C here does
    not cancel a stop that the app has accepted. A confirmed stop means the VM
    closed, not that Windows shut down normally.
    Stop JSON schema: bridgevm.app-stop.v2; scope: app-owned-runtime-control.
    Stop exit codes: 0 the VM stopped, 1 refused or not confirmed, 2 invalid usage.
    An accepted stop alone never exits 0.
    """
}

extension NativeCLI {
    static let startHelp = """
    Start a VM:
      start ID      Start a saved, installed Windows VM in the running app.
      bridgevm app start dev-vm --json

    Waits up to 30 seconds for the VM to start. Success means the VM is running;
    Windows may still be starting up. No window opens. BridgeVM must already be
    open with its library loaded. The TPM key is handled as for Start in the app:
    a new VM gets its key then. If an answer is lost, the command asks about the
    same start again. A timeout or Ctrl-C here does not cancel a start that the
    app has accepted.
    Start JSON schema: bridgevm.app-start.v2; scope: app-owned-runtime-start.
    Start exit codes: 0 the VM started, 1 refused or not confirmed, 2 invalid usage.
    """
}
