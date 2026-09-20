import Foundation

enum NativeCLISnapshotExport {
    static func run(
        rootURL: URL,
        options: NativeCLISnapshotExportOptions,
        repoRoot: URL = HvfEngineSession.defaultRepoRoot()
    ) -> NativeCLISnapshotResult {
        do {
            let resolved = NativeCLISnapshotExportOptions(
                id: options.id,
                output: resolvedOutput(options.output)
            )
            let config = try NativeLibraryReader.readConfig(rootURL: rootURL, id: options.id)
            guard config.backendKind == "hvf-engine" else { throw unavailable("Powered-off snapshot export requires the saved own-HVF backend.") }
            guard config.installPending != true else { throw unavailable("Windows installation must finish before snapshot export.") }
            guard let engine = HvfEngineConfig.libraryVM(config, rootURL: rootURL) else { throw unavailable("Cannot form the saved VM's HVF snapshot configuration.") }
            let plan = try HvfWindowsSnapshotCommand.plan(config: engine, repoRoot: repoRoot, operation: .create)
            let bundle = plan.disk.deletingLastPathComponent().deletingLastPathComponent().standardizedFileURL
            guard !resolved.output.pathComponents.starts(with: bundle.pathComponents) else { throw unavailable("Snapshot export output must be outside the managed VM bundle.") }
            _ = try HvfWindowsSnapshotCommand.invoke(
                plan.executable,
                ["create", plan.disk.path, plan.vars.path, resolved.output.path, plan.vmID, String(plan.quotaBytes)]
            )
            _ = try HvfWindowsSnapshotCommand.invoke(plan.executable, ["verify", resolved.output.path])
            return result(rootURL, resolved, complete: true, reason: nil)
        } catch {
            return result(rootURL, options, complete: false, reason: error.localizedDescription)
        }
    }

    private static func result(_ root: URL, _ options: NativeCLISnapshotExportOptions, complete: Bool, reason: String?) -> NativeCLISnapshotResult {
        NativeCLISnapshotResult(command: "export", vmID: options.id, libraryPath: root.path, snapshotPath: complete ? options.output.path : nil, complete: complete, unavailableReason: reason)
    }

    private static func unavailable(_ message: String) -> NativeCLIError { .unavailable(message) }

    private static func resolvedOutput(_ output: URL) -> URL {
        let output = output.standardizedFileURL
        var ancestor = output.deletingLastPathComponent()
        var suffix = [output.lastPathComponent]
        while !FileManager.default.fileExists(atPath: ancestor.path), ancestor.path != "/" {
            suffix.insert(ancestor.lastPathComponent, at: 0)
            ancestor.deleteLastPathComponent()
        }
        var resolved = ancestor.resolvingSymlinksInPath()
        for component in suffix { resolved.appendPathComponent(component, isDirectory: true) }
        return resolved.standardizedFileURL
    }
}

extension NativeCLI {
    static func executeSnapshotExport(_ options: NativeCLIOptions) throws -> Int32 {
        guard case .snapshotExport(let export) = options.command else { throw NativeCLIError.invalid("Expected snapshot-export ID OUTPUT.") }
        let result = NativeCLISnapshotExport.run(rootURL: options.libraryRoot, options: export)
        try output(result, json: options.json, text: result.text)
        return result.complete ? 0 : 1
    }
}
