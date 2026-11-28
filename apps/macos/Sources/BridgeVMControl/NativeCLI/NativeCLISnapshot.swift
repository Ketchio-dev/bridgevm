import Foundation

struct NativeCLISnapshotResult: Encodable {
    let schema = "bridgevm.app-snapshot.v1"
    let command: String
    let vmID: String
    let libraryPath: String
    let snapshotPath: String?
    let complete: Bool
    let unavailableReason: String?
    /// The helper's own error text, when the reason above is the app's sentence for it.
    var detail: String? = nil
    /// The SIGINT or SIGTERM that cancelled the helper; the exit status is 128 plus it.
    var cancelSignal: Int32? = nil

    var exitStatus: Int32 { cancelSignal.map { 128 + $0 } ?? (complete ? 0 : 1) }

    var text: String {
        var lines = [
            "Powered-off snapshot \(command): \(vmID)",
            "Result: \(complete ? "complete" : cancelSignal == nil ? "refused" : "cancelled")",
        ]
        if let snapshotPath { lines.append("Snapshot: \(snapshotPath)") }
        if let unavailableReason { lines.append("Reason: \(unavailableReason)") }
        lines.append("This host operation does not prove a Windows boot or guest-visible restore.")
        return lines.joined(separator: "\n") + "\n"
    }

    /// The app's sentence for a failure it knows (a running VM, no snapshot, a full drive…), with the
    /// helper's text kept as detail; BridgeVM's own refusals and other helper text pass through.
    static func reason(for error: Error, operation: HvfWindowsSnapshotCommand.Operation) -> (String, String?) {
        let raw = error.localizedDescription
        guard !(error is NativeCLIError),
              let plain = HvfWindowsSnapshotModel.knownMessage(for: error, operation: operation), plain != raw
        else { return (raw, nil) }
        return (plain, raw)
    }

    /// What a cancelled helper leaves: its old state or its complete result, never a mix.
    static func cancelled(_ cancel: HvfHelperProcess.Cancelled, command: String, vmID: String,
                          libraryPath: String) -> Self {
        let reason: String
        switch command {
        case "restore": reason = "Cancelled; the VM's disk and UEFI vars are either unchanged or fully restored from the snapshot."
        case "export": reason = "Cancelled; the VM is unchanged, and the export destination holds either its previous contents or a complete export."
        default: reason = "Cancelled; the VM's disk and UEFI vars are unchanged, and no partial snapshot was added."
        }
        return Self(command: command, vmID: vmID, libraryPath: libraryPath, snapshotPath: nil,
                    complete: false, unavailableReason: reason, cancelSignal: cancel.signal)
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
        case .delete: command = "delete"
        }
        do {
            let config = try NativeLibraryReader.readConfig(rootURL: rootURL, id: id)
            guard config.backendKind == "hvf-engine" else {
                throw NativeCLIError.unavailable("Powered-off snapshots require the saved own-HVF backend.")
            }
            guard config.installPending != true else {
                throw NativeCLIError.unavailable("Windows installation must finish before snapshot operations.")
            }
            let engine = try NativeCLISnapshotAdmission.engine(config, rootURL: rootURL)
            var plan = try HvfWindowsSnapshotCommand.plan(
                config: engine,
                repoRoot: repoRoot,
                operation: operation
            )
            if operation == .create {
                plan = try HvfWindowsSnapshotCommand.create(plan: plan, arguments: plan.arguments(for: .create))
                _ = HvfWindowsSnapshotCommand.nameCreated(plan)
            } else if operation == .delete {
                try HvfWindowsSnapshotStore.delete(plan.snapshot, bundle: plan.bundle)
            } else {
                try HvfWindowsSnapshotCommand.restore(plan: plan, arguments: plan.arguments(for: operation))
            }
            return NativeCLISnapshotResult(
                command: command,
                vmID: id,
                libraryPath: rootURL.path,
                snapshotPath: plan.snapshot.path,
                complete: true,
                unavailableReason: nil
            )
        } catch let cancel as HvfHelperProcess.Cancelled {
            return .cancelled(cancel, command: command, vmID: id, libraryPath: rootURL.path)
        } catch {
            let (reason, detail) = NativeCLISnapshotResult.reason(for: error, operation: operation)
            return NativeCLISnapshotResult(command: command, vmID: id, libraryPath: rootURL.path, snapshotPath: nil,
                                           complete: false, unavailableReason: reason, detail: detail)
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
        return result.exitStatus
    }
}

/// Snapshot commands run without the app's library admission. The app does not
/// load a VM with a relocation record (VMLibrary.scan), so no app snapshot can
/// reach it; refuse here too, before any directory, lease or helper work.
enum NativeCLISnapshotAdmission {
    static func engine(_ config: VMConfig, rootURL: URL) throws -> HvfEngineConfig {
        guard !VMRelocationJournal.isPending(config, rootURL: rootURL) else {
            throw NativeCLIError.unavailable(
                "VM relocation recovery is unresolved. Check both bundles and the registration before snapshot operations.")
        }
        guard let engine = HvfEngineConfig.libraryVM(config, rootURL: rootURL) else {
            throw NativeCLIError.unavailable("Cannot form the saved VM's HVF snapshot configuration.")
        }
        return engine
    }
}

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
            let engine = try NativeCLISnapshotAdmission.engine(config, rootURL: rootURL)
            let plan = try HvfWindowsSnapshotCommand.plan(config: engine, repoRoot: repoRoot, operation: .create)
            let bundle = plan.disk.deletingLastPathComponent().deletingLastPathComponent().standardizedFileURL
            guard !resolved.output.pathComponents.starts(with: bundle.pathComponents) else { throw unavailable("Snapshot export output must be outside the managed VM bundle.") }
            // Create hashes both members into the manifest; restore verifies them before use.
            _ = try HvfWindowsSnapshotCommand.invoke(plan.executable, plan.createArguments(destination: resolved.output))
            return result(rootURL, resolved, complete: true, reason: nil)
        } catch let cancel as HvfHelperProcess.Cancelled {
            return .cancelled(cancel, command: "export", vmID: options.id, libraryPath: rootURL.path)
        } catch {
            let (reason, detail) = NativeCLISnapshotResult.reason(for: error, operation: .create)
            var refused = result(rootURL, options, complete: false, reason: reason)
            refused.detail = detail
            return refused
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
        return result.exitStatus
    }
}

struct NativeCLISnapshotExportOptions: Equatable {
    let id: String
    let output: URL
}

enum NativeCLISnapshotExportParser {
    static func selectsCommand(_ arguments: [String]) -> Bool {
        var skipValue = false
        for argument in arguments {
            if skipValue { skipValue = false; continue }
            if argument == "--library" { skipValue = true; continue }
            if argument.hasPrefix("-") { continue }
            return argument == "snapshot-export"
        }
        return false
    }

    static func parse(arguments: [String], defaultLibrary: URL) throws -> NativeCLIOptions {
        var positionals: [String] = [], library: URL?
        var json = false, help = false, index = 0
        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "--library":
                guard library == nil, index + 1 < arguments.count else { throw invalid("--library requires one absolute path and may appear only once.") }
                index += 1
                library = try absolute(arguments[index], label: "--library")
            case "--json":
                guard !json else { throw invalid("--json may appear only once.") }
                json = true
            case "--help", "-h": help = true
            default:
                guard !argument.hasPrefix("-") else { throw invalid("Unknown option: \(argument). Run `bridgevm app --help`.") }
                positionals.append(argument)
            }
            index += 1
        }
        if help, positionals.first == "snapshot-export" {
            return NativeCLIOptions(command: .snapshotExport(.init(id: "", output: URL(fileURLWithPath: "/"))), libraryRoot: library ?? defaultLibrary, json: json, showHelp: true)
        }
        guard positionals.count == 3, positionals[0] == "snapshot-export" else { throw invalid("Expected snapshot-export ID OUTPUT. Use --cli --help.") }
        guard NativeLibraryReader.isCanonicalID(positionals[1]) else { throw invalid(NativeLibraryReader.notAnID(positionals[1])) }
        let output = try absolute(positionals[2], label: "OUTPUT")
        guard output.path != "/" else { throw invalid("OUTPUT must name an export directory.") }
        return NativeCLIOptions(command: .snapshotExport(.init(id: positionals[1], output: output)), libraryRoot: library ?? defaultLibrary, json: json, showHelp: false)
    }

    private static func absolute(_ path: String, label: String) throws -> URL {
        guard path.hasPrefix("/"), !(path as NSString).pathComponents.contains("..") else { throw invalid("\(label) requires an absolute path without '..'.") }
        return URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
    }

    private static func invalid(_ message: String) -> NativeCLIError { .invalid(message) }
}
