import Foundation

/// A guest workload's share output. When it never arrives the timeout says what
/// the guest reported: a failed action's exception type and any stage note.
enum T17GuestWorkloadOutput {
    static func await(share: URL, name: String, prefix: String, timeout: TimeInterval, note: () -> String? = { nil }) throws {
        let output = share.appendingPathComponent(name), deadline = Date().addingTimeInterval(timeout)
        while !FileManager.default.fileExists(atPath: output.path) {
            guard Date() < deadline else {
                let failed = failure(share.appendingPathComponent("t17-error-\(prefix).txt")).map { "guest workload failed: \($0)" }
                throw T17Blocker(code: "guest-evidence-missing", detail: "guest workload did not produce \(name)"
                    + [note(), failed].compactMap { $0.map { " (\($0))" } }.joined())
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
        let values = try output.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size < 8 * 1024 * 1024 else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest output is unsafe or oversized")
        }
    }

    /// Guest-written, so only an exact action name and exception type name is reported.
    static func failure(_ url: URL) -> String? {
        guard T17BoundedLog.regularFile(url), let data = try? Data(contentsOf: url), data.count <= 160,
              let text = String(data: data, encoding: .utf8), text.hasSuffix("\n") else { return nil }
        let line = String(text.dropLast())
        return line.range(of: #"^action=[A-Za-z]{1,32} error=[A-Za-z_][A-Za-z0-9_.`+]{0,119}$"#, options: .regularExpression) == nil ? nil : line
    }
}
