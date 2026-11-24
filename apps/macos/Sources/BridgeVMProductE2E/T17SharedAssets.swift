import Foundation

/// A new guest receives the scripts from the host. A rebooted or imported guest
/// can already have them, in which case the share need not transfer them again.
enum T17SharedAssets {
    static func wait(
        _ names: [String], in runLog: URL, nonce: String, timeout: TimeInterval,
        enter: (String) throws -> Void
    ) -> Bool {
        let ready = "T17-SHARED-ASSETS-\(nonce)-ready"
        let missing = "T17-SHARED-ASSETS-\(nonce)-missing"
        let checks = names.map { "(Test-Path -LiteralPath 'C:\\bridgevm-share\\\($0)' -PathType Leaf)" }
            .joined(separator: " -and ")
        let command = "powershell.exe -NoProfile -NonInteractive -Command \"if (\(checks)) { '\(ready)' } else { '\(missing)' }\""
        let deadline = Date().addingTimeInterval(timeout)
        var pending: Int?
        repeat {
            var delay: TimeInterval = 0.2
            if let offset = pending {
                let tail = T17TPMRunLog.text(runLog, after: offset)
                if T17ControlReply.succeeded(tail: tail, command: command, marker: ready) { return true }
                if T17ControlReply.succeeded(tail: tail, command: command, marker: missing) {
                    pending = nil
                    // Give the share a chance to run between short guest commands.
                    delay = 2
                }
            } else {
                let log = T17TPMRunLog.text(runLog)
                if names.allSatisfy({ log.contains("BVAGENT SHARE host->guest \($0) bytes=") }) { return true }
                if log.split(whereSeparator: \.isNewline).contains(where: { $0.hasPrefix("BVAGENT SERVICE start") }) {
                    pending = T17TPMRunLog.size(runLog)
                    guard (try? enter(command)) != nil else { return false }
                }
            }
            RunLoop.current.run(until: min(deadline, Date().addingTimeInterval(delay)))
        } while Date() < deadline
        return false
    }
}
