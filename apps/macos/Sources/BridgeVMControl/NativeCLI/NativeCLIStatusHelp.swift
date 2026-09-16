extension NativeCLI {
    static let statusHelp = """
    Runtime observation:
      status ID     Query the running native app's retained own-HVF session.
      BridgeVMControl --cli status 개발-vm --json

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
