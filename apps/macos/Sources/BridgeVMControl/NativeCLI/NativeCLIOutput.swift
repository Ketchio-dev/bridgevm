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
        var lines = ["Native VM readiness: \(snapshot.id)", "Runtime state: \(snapshot.runtimeState)",
                     "Launch prerequisites: \(snapshot.launchReady ? "ready" : "blocked")"]
        if !snapshot.engineChecksPerformed {
            lines.append("Engine prerequisite and product release checks were not evaluated.")
        }
        for issue in snapshot.launchBlockers {
            lines.append("Launch blocker [\(issue.code)]: \(issue.summary)")
        }
        for issue in snapshot.releaseBlockers {
            lines.append("Product release gate [\(issue.code)]: \(issue.summary)")
        }
        for limitation in snapshot.productLimitations { lines.append("Limitation: \(limitation)") }
        for observation in snapshot.recoveryObservations { lines.append("Recovery: \(observation)") }
        lines.append("Prerequisite query only; no VM boot or product release result was observed.")
        return lines.joined(separator: "\n") + "\n"
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

    static func write(_ text: String, to handle: FileHandle) {
        handle.write(Data(text.utf8))
    }
}
