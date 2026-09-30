import Foundation

/// The input challenge's share files: the form's ready marker, its output, and
/// the progress line that says what the form saw when no output arrives.
enum T17InputChallengeShare {
    static func waitUntilShown(share: URL, nonce: String, timeout: TimeInterval) throws {
        let marker = share.appendingPathComponent("t17-keyboard-pointer-ready-\(prefix(nonce)).txt")
        let body = Data("bridgevm-t17-keyboard-pointer-ready-v1\n\(nonce)\n".utf8)
        let deadline = Date().addingTimeInterval(timeout)
        while !(T17BoundedLog.regularFile(marker) && (try? Data(contentsOf: marker)) == body) {
            guard Date() < deadline else {
                let present = (try? FileManager.default.attributesOfItem(atPath: marker.path)) != nil
                throw T17Blocker(code: "guest-evidence-missing", detail: present
                    ? "guest input challenge ready marker is unsafe or changed" : "guest input challenge was not shown")
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
    }

    static func awaitOutput(share: URL, nonce: String, timeout: TimeInterval) throws {
        let name = "t17-keyboard-pointer-\(prefix(nonce)).txt", output = share.appendingPathComponent(name)
        let deadline = Date().addingTimeInterval(timeout)
        while !FileManager.default.fileExists(atPath: output.path) {
            guard Date() < deadline else {
                let seen = progress(share.appendingPathComponent("t17-keyboard-pointer-progress-\(prefix(nonce)).txt"))
                throw T17Blocker(code: "guest-evidence-missing", detail: "guest workload did not produce \(name)"
                    + (seen.map { " (guest form saw \($0))" } ?? ""))
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
        let values = try output.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size < 8 * 1024 * 1024 else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest output is unsafe or oversized")
        }
    }

    /// Guest-written, so only an exact `clicked=<0|1> typed=<0-999>` line is reported.
    static func progress(_ url: URL) -> String? {
        guard T17BoundedLog.regularFile(url), let data = try? Data(contentsOf: url), data.count <= 32,
              let text = String(data: data, encoding: .utf8), text.hasSuffix("\n") else { return nil }
        let line = String(text.dropLast())
        return line.range(of: #"^clicked=[01] typed=[0-9]{1,3}$"#, options: .regularExpression) == nil ? nil : line
    }

    private static func prefix(_ nonce: String) -> String { String(nonce.prefix(12)) }
}
