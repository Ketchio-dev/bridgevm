import Foundation

enum NativeCLI {
    static func run(arguments: [String]) -> Int32 {
        do {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            if options.showHelp {
                write(help + "\n", to: .standardOutput)
                return 0
            }
            return try execute(options)
        } catch {
            write("BridgeVM: \(error.localizedDescription)\n", to: .standardError)
            if case NativeCLIError.invalid = error { return 2 }
            return 1
        }
    }
}

enum NativeCLIError: LocalizedError {
    case invalid(String), unavailable(String)
    var errorDescription: String? {
        switch self { case .invalid(let text), .unavailable(let text): return text }
    }
}

/// Text output names the common failures in a sentence (the most common, BridgeVM not open, printed
/// `ownerUnavailable`); --json keeps the codes. Other reasons, including app sentences, pass through.
enum NativeCLIReason {
    static func sentence(_ reason: String, vmID: String, action: String) -> String {
        switch reason {
        case "ownerUnavailable": return "BridgeVM isn’t open for this library. Open BridgeVM, then try again."
        case "ownerBusy": return "BridgeVM is busy with another request. Try again in a moment."
        case "timedOut": return "BridgeVM didn’t answer in time. Try again."
        case "libraryChanged", "configurationChanged": return "The VM library changed meanwhile. Try again."
        case "deadlineExceeded":
            return "BridgeVM is still \(action) the VM. Run `bridgevm app status \(vmID)` to follow it."
        case "modelUnavailable": return "BridgeVM is still opening this library. Try again in a moment."
        case "busy": return "Another operation is using this VM. Try again when it finishes."
        case "unsupportedTarget": return "This VM isn’t waiting to be installed."
        case "targetUnavailable": return "BridgeVM isn’t installing this VM."
        case "cancellationUnavailable": return "The installation can no longer be canceled."
        default: return reason
        }
    }
}

extension NativeCLI {
    static func execute(_ options: NativeCLIOptions) throws -> Int32 {
        switch options.command {
        case .start, .stop: return try executeRuntime(options)
        case .install, .installStatus, .installCancel: return try executeInstall(options)
        case .snapshotCreate, .snapshotRestore: return try executeSnapshot(options)
        case .snapshotExport: return try executeSnapshotExport(options)
        case .status(let id):
            let snapshot = NativeCLIRuntimeStatus.snapshot(rootURL: options.libraryRoot, id: id)
            try output(snapshot, json: options.json, text: snapshot.text); return snapshot.complete ? 0 : 1
        case .readiness(let id):
            let snapshot = try NativeCLIReadiness.snapshot(rootURL: options.libraryRoot, id: id)
            try output(snapshot, json: options.json, text: render(snapshot))
            return snapshot.launchReady ? 0 : 1
        case .list, .inspect, .createWindows:
            if case .createWindows = options.command { return try executeCreateWindows(options) }
            let id: String?
            if case .inspect(let value) = options.command { id = value } else { id = nil }
            let snapshot = try NativeLibraryReader.snapshot(rootURL: options.libraryRoot, id: id)
            try output(snapshot, json: options.json, text: render(snapshot)); return snapshot.complete ? 0 : 1
        }
    }
}

extension NativeCLI {
    static func executeRuntime(_ options: NativeCLIOptions) throws -> Int32 {
        switch options.command {
        case .start(let id):
            let result = NativeCLIRuntimeStart.run(rootURL: options.libraryRoot, id: id)
            try output(result, json: options.json, text: result.text)
            return result.started ? 0 : 1
        case .stop(let id):
            let result = NativeCLIRuntimeStop.run(rootURL: options.libraryRoot, id: id)
            try output(result, json: options.json, text: result.text)
            return result.complete ? 0 : 1
        default: throw NativeCLIError.invalid("Expected a runtime command.")
        }
    }
}

extension NativeCLI {
    static func executeInstall(_ options: NativeCLIOptions) throws -> Int32 {
        let id: String, operation: NativeInstallControlRequest.Operation
        switch options.command {
        case let .install(value): id = value; operation = .install
        case let .installStatus(value): id = value; operation = .installStatus
        case let .installCancel(value): id = value; operation = .installCancel
        default: throw NativeCLIError.invalid("Expected an install command.")
        }
        let result = NativeCLIInstall.run(rootURL: options.libraryRoot, id: id, operation: operation)
        try output(result, json: options.json, text: result.text)
        return result.complete ? 0 : 1
    }
}

extension NativeCLI {
    static func output<Value: Encodable>(_ value: Value, json: Bool, text: String) throws {
        if json {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            FileHandle.standardOutput.write(try encoder.encode(value))
            write("\n", to: .standardOutput)
        } else {
            write(text, to: .standardOutput)
        }
    }

    static func render(_ snapshot: NativeCLIReadiness) -> String {
        let value = NativeCLITextValue.escaped
        var lines = ["Native VM readiness: \(value(snapshot.id))", "Runtime state: \(value(snapshot.runtimeState))",
                     "Launch prerequisites: \(snapshot.launchReady ? "ready" : "blocked")"]
        if !snapshot.engineChecksPerformed {
            lines.append("Engine prerequisite and product release checks were not evaluated.")
        }
        for issue in snapshot.launchBlockers {
            lines.append("Launch blocker [\(value(issue.code))]: \(value(issue.summary))")
        }
        for issue in snapshot.releaseBlockers {
            lines.append("Product release gate [\(value(issue.code))]: \(value(issue.summary))")
        }
        for limitation in snapshot.productLimitations { lines.append("Limitation: \(value(limitation))") }
        for observation in snapshot.recoveryObservations { lines.append("Recovery: \(value(observation))") }
        lines.append("Prerequisite query only; no VM boot or product release result was observed.")
        return lines.joined(separator: "\n") + "\n"
    }

    static func render(_ snapshot: NativeLibrarySnapshot) -> String {
        let value = NativeCLITextValue.escaped
        var lines = ["Native VM library: \(value(snapshot.libraryPath))", "Runtime state: unobserved"]
        if snapshot.records.isEmpty { lines.append("No readable saved VMs.") }
        for record in snapshot.records {
            lines.append("\(value(record.id))\t\(NativeCLITextValue.quoted(record.displayName))\t\(value(record.backendKind))")
            lines.append("  Configuration: \(value(record.configPath))")
            lines.append("  Bundle: \(value(record.bundlePath))")
            lines.append("  Saved resources: \(record.cpuCount.map(String.init) ?? "default") CPU, \(record.memoryMiB.map(String.init) ?? "default") MiB")
            lines.append("  Installation pending: \(record.installPending.map(String.init) ?? "unspecified")")
            for observation in record.recoveryObservations { lines.append("  Recovery: \(value(observation))") }
        }
        for issue in snapshot.issues { lines.append("Issue [\(value(issue.code))]: \(value(issue.path)): \(value(issue.message))") }
        return lines.joined(separator: "\n") + "\n"
    }

    static func write(_ text: String, to handle: FileHandle) {
        handle.write(Data(text.utf8))
    }
}

/// Escape values, not the renderer's own line and column separators. JSON stays lossless.
enum NativeCLITextValue {
    static func escaped(_ text: String) -> String {
        var result = ""
        for scalar in text.unicodeScalars {
            switch scalar.value {
            case 0x08: result += "\\b"
            case 0x09: result += "\\t"
            case 0x0a: result += "\\n"
            case 0x0c: result += "\\f"
            case 0x0d: result += "\\r"
            case 0x00...0x1f, 0x7f...0x9f, 0x061c, 0x200e...0x200f,
                 0x2028...0x202e, 0x2066...0x206f:
                result += String(format: "\\u%04x", scalar.value)
            default: result.unicodeScalars.append(scalar)
            }
        }
        return result
    }

    static func quoted(_ text: String) -> String {
        guard let data = try? JSONEncoder().encode(text) else { return "\"\"" }
        // JSON already escapes quotes and C0; also neutralize C1 and bidi controls.
        return escaped(String(decoding: data, as: UTF8.self))
    }
}
