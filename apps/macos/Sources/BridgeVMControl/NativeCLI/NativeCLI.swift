import Foundation

enum NativeCLI {
    static let help = """
    BridgeVM native app CLI

    Usage: BridgeVMControl --cli <command> [--library ABSOLUTE_PATH] [--json]
      list          List saved VMs in the native app library.
      inspect ID    Inspect one exact VM ID from list (including Korean IDs).

    Examples:
      BridgeVMControl --cli list --json
      BridgeVMControl --cli inspect 개발-vm

    Uses the same vm.json configuration as the native app. Reads never repair,
    migrate, create or start a VM. Runtime state is unobserved. Recovery records
    are reported without interpreting or modifying them. CPU/memory are saved
    values, not measured usage. JSON schema: bridgevm.app-library.v1.
    Exit codes: 0 complete query, 1 unavailable/incomplete inventory, 2 invalid usage.
    Legacy Rust CLI --store/--socket commands use a separate manifest.yaml store.
    """

    static func run(arguments: [String]) -> Int32 {
        do {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            if options.showHelp {
                write(help + "\n", to: .standardOutput)
                return 0
            }
            let id: String?
            switch options.command { case .list: id = nil; case .inspect(let value): id = value }
            let snapshot = try NativeLibraryReader.snapshot(rootURL: options.libraryRoot, id: id)
            if options.json {
                let encoder = JSONEncoder()
                encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
                FileHandle.standardOutput.write(try encoder.encode(snapshot))
                write("\n", to: .standardOutput)
            } else {
                write(render(snapshot), to: .standardOutput)
            }
            return snapshot.complete ? 0 : 1
        } catch {
            write("BridgeVM: \(error.localizedDescription)\n", to: .standardError)
            if case NativeCLIError.invalid = error { return 2 }
            return 1
        }
    }

    static func render(_ snapshot: NativeLibrarySnapshot) -> String {
        var lines = ["Native VM library: \(snapshot.libraryPath)", "Runtime state: unobserved"]
        if snapshot.records.isEmpty { lines.append("No readable saved VMs.") }
        for record in snapshot.records {
            lines.append("\(record.id)\t\(quoted(record.displayName))\t\(record.backendKind)")
            lines.append("  Configuration: \(record.configPath)")
            lines.append("  Bundle: \(record.bundlePath)")
            lines.append("  Saved resources: \(record.cpuCount.map(String.init) ?? "default") CPU, \(record.memoryMiB.map(String.init) ?? "default") MiB")
            lines.append("  Installation pending: \(record.installPending.map(String.init) ?? "unspecified")")
            for observation in record.recoveryObservations { lines.append("  Recovery: \(observation)") }
        }
        for issue in snapshot.issues { lines.append("Issue [\(issue.code)]: \(issue.path): \(issue.message)") }
        return lines.joined(separator: "\n") + "\n"
    }

    private static func quoted(_ text: String) -> String {
        guard let data = try? JSONEncoder().encode(text) else { return "\"\"" }
        return String(decoding: data, as: UTF8.self)
    }

    private static func write(_ text: String, to handle: FileHandle) {
        handle.write(Data(text.utf8))
    }
}
