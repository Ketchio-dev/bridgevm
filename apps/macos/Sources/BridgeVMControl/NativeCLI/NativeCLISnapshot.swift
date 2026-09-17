import Foundation

struct NativeCLISnapshotResult: Encodable {
    let schema = "bridgevm.app-snapshot.v1"
    let command: String
    let vmID: String
    let libraryPath: String
    let snapshotPath: String?
    let complete: Bool
    let unavailableReason: String?

    var text: String {
        var lines = [
            "Powered-off snapshot \(command): \(vmID)",
            "Result: \(complete ? "complete" : "refused")",
        ]
        if let snapshotPath { lines.append("Snapshot: \(snapshotPath)") }
        if let unavailableReason { lines.append("Reason: \(unavailableReason)") }
        lines.append("This host operation does not prove a Windows boot or guest-visible restore.")
        return lines.joined(separator: "\n") + "\n"
    }
}

enum NativeCLISnapshot {
    static func run(
        rootURL: URL,
        id: String,
        operation: HvfWindowsSnapshotCommand.Operation,
        repoRoot: URL = HvfEngineSession.defaultRepoRoot()
    ) -> NativeCLISnapshotResult {
        let command: String
        switch operation {
        case .create: command = "create"
        case .restore: command = "restore"
        }
        do {
            let config = try NativeLibraryReader.readConfig(rootURL: rootURL, id: id)
            guard config.backendKind == "hvf-engine" else {
                throw NativeCLIError.unavailable("Powered-off snapshots require the saved own-HVF backend.")
            }
            guard config.installPending != true else {
                throw NativeCLIError.unavailable("Windows installation must finish before snapshot operations.")
            }
            guard let engine = HvfEngineConfig.libraryVM(config, rootURL: rootURL) else {
                throw NativeCLIError.unavailable("Cannot form the saved VM's HVF snapshot configuration.")
            }
            let plan = try HvfWindowsSnapshotCommand.plan(
                config: engine,
                repoRoot: repoRoot,
                operation: operation
            )
            if case .create = operation {
                try FileManager.default.createDirectory(
                    at: plan.snapshot.deletingLastPathComponent(),
                    withIntermediateDirectories: true
                )
            }
            _ = try HvfWindowsSnapshotCommand.invoke(
                plan.executable,
                plan.arguments(for: operation)
            )
            if case .create = operation {
                _ = try HvfWindowsSnapshotCommand.invoke(
                    plan.executable,
                    ["verify", plan.snapshot.path]
                )
            }
            return NativeCLISnapshotResult(
                command: command,
                vmID: id,
                libraryPath: rootURL.path,
                snapshotPath: plan.snapshot.path,
                complete: true,
                unavailableReason: nil
            )
        } catch {
            return NativeCLISnapshotResult(
                command: command,
                vmID: id,
                libraryPath: rootURL.path,
                snapshotPath: nil,
                complete: false,
                unavailableReason: error.localizedDescription
            )
        }
    }
}

extension NativeCLI {
    static func executeSnapshot(_ options: NativeCLIOptions) throws -> Int32 {
        let id: String
        let operation: HvfWindowsSnapshotCommand.Operation
        switch options.command {
        case .snapshotCreate(let value): (id, operation) = (value, .create)
        case .snapshotRestore(let value): (id, operation) = (value, .restore)
        default: throw NativeCLIError.invalid("Expected a powered-off snapshot command.")
        }
        let result = NativeCLISnapshot.run(
            rootURL: options.libraryRoot,
            id: id,
            operation: operation
        )
        try output(result, json: options.json, text: result.text)
        return result.complete ? 0 : 1
    }
}
