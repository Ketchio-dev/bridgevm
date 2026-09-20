extension NativeCLI {
    static let snapshotHelp = """
    Powered-off snapshot commands:
      snapshot-create ID   Save the current disk and UEFI vars as one verified pair.
      snapshot-restore ID  Restore that verified pair for the powered-off VM.
      snapshot-export ID OUTPUT  Export the current verified raw pair to OUTPUT.
    The saved VM must use the own-HVF backend and must not be installation-pending
    or running. OUTPUT is an absolute directory outside the VM bundle. Media and
    helper paths come from the saved app library; disk, vars and secrets stay off argv.
    JSON schema: bridgevm.app-snapshot.v1.
    Exit codes: 0 complete, 1 refused/failed, 2 invalid usage.
    """
}
