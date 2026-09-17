extension NativeCLI {
    static let snapshotHelp = """
    Powered-off snapshot commands:
      snapshot-create ID   Save the current disk and UEFI vars as one verified pair.
      snapshot-restore ID  Restore that verified pair for the powered-off VM.

    The saved VM must use the own-HVF backend and must not be installation-pending
    or running. The media and helper paths come only from the saved app library;
    disk paths, vars paths and secrets are not accepted on argv.
    JSON schema: bridgevm.app-snapshot.v1.
    Exit codes: 0 complete, 1 refused/failed, 2 invalid usage.
    """
}
