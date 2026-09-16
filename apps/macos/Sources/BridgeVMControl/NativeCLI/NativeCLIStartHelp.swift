extension NativeCLI {
    static let startHelp = """
    Owned runtime start:
      start ID      Start one saved, installed own-HVF VM in the running app.
      BridgeVMControl --cli start 개발-vm --json

    Observes startup for up to 30 seconds. Success requires the supervisor READY
    message and initial helper startup; it does not prove guest boot or display.
    The app and its library model must already be running. No app/window opens.
    Requires an existing accessible TPM key; no key is created or prompt requested.
    Exact retries reuse one operation and saved configuration. A deadline or CLI
    disconnect does not release pending app-owned work or start a replacement.
    Start JSON schema: bridgevm.app-start.v1; scope: app-owned-runtime-start.
    Start exit codes: 0 startup confirmed, 1 refused/unconfirmed, 2 invalid usage.
    """
}
