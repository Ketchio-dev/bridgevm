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
    JSON schema: bridgevm.app-install-command.v1.
    """
}
