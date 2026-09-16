extension NativeCLI {
    static let help = """
    BridgeVM native app CLI (bridgevm-native-cli-v1)

    Usage: BridgeVMControl --cli <command> [--library ABSOLUTE_PATH] [--json]
      list          List saved VMs in the native app library.
      inspect ID    Inspect one exact VM ID from list (including Korean IDs).
      readiness ID  Check saved own-HVF VM launch prerequisites without starting it.

    Examples:
      BridgeVMControl --cli list --json
      BridgeVMControl --cli inspect 개발-vm
      BridgeVMControl --cli readiness 개발-vm --json

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
    Legacy Rust CLI --store/--socket commands use a separate manifest.yaml store.
    """ + "\n\n" + statusHelp + "\n\n" + installHelp + "\n\n" + stopHelp + "\n\n" + startHelp
}
