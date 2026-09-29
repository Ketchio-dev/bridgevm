import CryptoKit
import Foundation

enum T17FirstBootDiagnostic {
    static let captureLimit = 8 * 1024 * 1024
    private static let markers: [(String, (Substring) -> Bool)] = [
        ("ready", { $0.contains("BVAGENT READY") }),
        ("service_start", { $0.contains("BVAGENT SERVICE start") }),
        ("system_reset", { $0.hasPrefix(HvfStopLine.systemResetPrefix) }),
        ("system_off", { $0 == HvfStopLine.systemOff }),
        ("console_stats", { $0.contains("virtio-console stats") }),
    ]

    static func capture(_ url: URL, maxBytes: Int = captureLimit) -> String {
        guard maxBytes > 0 else { return "run_log=status=invalid-limit" }
        let keys: Set<URLResourceKey> = [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]
        guard let values = try? url.resourceValues(forKeys: keys) else {
            return "run_log=status=absent"
        }
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let handle = FileHandle(forReadingAtPath: url.path) else {
            return "run_log=status=unsafe"
        }
        defer { try? handle.close() }
        do {
            let total = try handle.seekToEnd()
            let captured = min(total, UInt64(maxBytes))
            try handle.seek(toOffset: total - captured)
            let data = try handle.read(upToCount: Int(captured)) ?? Data()
            guard data.count == Int(captured) else { return "run_log=status=short-read" }
            return summarize(data, totalBytes: total)
        } catch {
            return "run_log=status=read-failed"
        }
    }

    static func summarize(_ data: Data, totalBytes: UInt64) -> String {
        let state = totalBytes == 0 ? "empty" : "present"
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        let lines = String(decoding: data, as: UTF8.self).split(whereSeparator: \.isNewline)
        let counts = markers.map { name, matches in
            "\(name)=\(lines.filter(matches).count)"
        }.joined(separator: ",")
        let truncated = UInt64(data.count) < totalBytes ? 1 : 0
        return "run_log=status=\(state),bytes=\(totalBytes),captured=\(data.count),truncated=\(truncated),sha256=\(digest),\(counts)"
    }
}
