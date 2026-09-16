extension NativeCLI {
    static let stopHelp = """
    Owned runtime stop:
      stop ID       Ask the running app to stop its exact retained owned run.
      BridgeVMControl --cli stop 개발-vm --json

    Waits up to 210 seconds for supervisor cleanup and retained runner exit.
    Requests reuse the same target and operation; retries do not reset the stop
    deadline. Attached observations and process-name matches cannot be stopped.
    The app must already be running. No app or VM is launched by this command.
    A timeout ends this CLI observation; admitted cleanup remains app-owned.
    Confirmed runtime cleanup does not prove normal guest shutdown.
    Stop JSON schema: bridgevm.app-stop.v1; scope: app-owned-runtime-control.
    Stop exit codes: 0 cleanup and runner exit confirmed, 1 refused/unconfirmed,
    2 invalid usage. Pending admission alone never returns a successful exit.
    """
}
