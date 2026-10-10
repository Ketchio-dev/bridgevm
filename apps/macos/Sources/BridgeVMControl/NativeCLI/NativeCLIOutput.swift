import Foundation

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
