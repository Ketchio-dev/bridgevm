import Darwin
import Foundation
import Security

/// Runs after first-READY has already failed. Only status and a generation
/// number leave this method; raw guest bytes stay in the owned lane.
enum T17FirstReadyStopCapture {
    static let requestName = "diagnostic-stop.request"
    static let pendingName = "diagnostic-stop.request.pending"
    static let maxReportBytes = 16 * 1024 * 1024

    private struct LogIdentity {
        let device: dev_t
        let inode: ino_t
        let offset: UInt64
    }

    static func capture(log: URL, laneRoot: URL, applicationRunning: () -> Bool,
                        ownedRuntimeState: () -> String?, timeout: TimeInterval = 30,
                        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
                        pause: () -> Void = { Thread.sleep(forTimeInterval: 0.2) }) -> String {
        guard applicationRunning() else { return "host_stop=status=missing,reason=app-exited" }
        let lane = laneRoot.resolvingSymlinksInPath().standardizedFileURL.path
        let evidence = log.deletingLastPathComponent().resolvingSymlinksInPath().standardizedFileURL.path
        guard evidence.hasPrefix(lane + "/"), log.lastPathComponent == "run.log" else {
            return "host_stop=status=missing,reason=outside-owned-lane"
        }
        guard let identity = inspect(log) else { return "host_stop=status=missing,reason=unsafe-run-log" }
        guard let nonce = freshNonce() else { return "host_stop=status=missing,reason=host-rng-failed" }
        let request = log.deletingLastPathComponent().appendingPathComponent(requestName)
        let pending = log.deletingLastPathComponent().appendingPathComponent(pendingName)
        let fd = Darwin.open(pending.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0o600)
        guard fd >= 0 else { return "host_stop=status=missing,reason=request-create-failed" }
        let payload = Data("t17-nonce-v1:\(nonce)\n".utf8)
        let written = payload.withUnsafeBytes { Darwin.write(fd, $0.baseAddress, $0.count) }
        let modeSet = Darwin.fchmod(fd, 0o600) == 0
        let synced = Darwin.fsync(fd) == 0
        let closed = Darwin.close(fd)
        guard written == payload.count && modeSet && synced && closed == 0 else {
            _ = Darwin.unlink(pending.path)
            return "host_stop=status=incomplete,reason=request-seal-failed"
        }
        guard Darwin.renameatx_np(AT_FDCWD, pending.path, AT_FDCWD, request.path, UInt32(RENAME_EXCL)) == 0 else {
            _ = Darwin.unlink(pending.path)
            return "host_stop=status=missing,reason=request-create-failed"
        }
        let ackPattern = try? NSRegularExpression(pattern: "(?m)^HOST-DIAGNOSTIC-STOP: generation=([0-9]+) nonce=\(nonce) request consumed; ending run through final report$")
        let deadline = now() + timeout
        var observedAck = false
        var observedReport = false
        repeat {
            guard let rawSuffix = readSuffix(log, identity: identity) else {
                return "host_stop=status=incomplete,reason=run-log-changed"
            }
            let suffix = String(decoding: rawSuffix, as: UTF8.self)
            if let match = ackPattern?.firstMatch(in: suffix, range: NSRange(suffix.startIndex..., in: suffix)),
               Range(match.range, in: suffix) != nil,
               let generationRange = Range(match.range(at: 1), in: suffix),
               let generation = UInt64(suffix[generationRange]) {
                observedAck = true
                if T17TerminalReportTail.isComplete(rawSuffix: rawSuffix, nonce: nonce, generation: generation) {
                    observedReport = true
                    if ownedRuntimeState() == "stopped" {
                        guard requestConsumed(request) else {
                            return "host_stop=status=incomplete,reason=request-not-consumed"
                        }
                        return "host_stop=status=complete,generation=\(generation),nonce=\(nonce),report=complete,helper=terminal,log_offset=\(identity.offset)"
                    }
                }
            }
            if !applicationRunning() { return "host_stop=status=incomplete,reason=app-exited" }
            pause()
        } while now() < deadline
        if observedReport { return "host_stop=status=incomplete,reason=helper-not-terminal" }
        if observedAck { return "host_stop=status=incomplete,reason=final-report-missing" }
        return "host_stop=status=missing,reason=acknowledgement-missing"
    }

    private static func freshNonce() -> String? {
        var bytes = [UInt8](repeating: 0, count: 16)
        let status = bytes.withUnsafeMutableBytes {
            SecRandomCopyBytes(kSecRandomDefault, $0.count, $0.baseAddress!)
        }
        guard status == errSecSuccess else { return nil }
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    private static func requestConsumed(_ url: URL) -> Bool {
        var state = stat()
        return Darwin.lstat(url.path, &state) == -1 && errno == ENOENT
    }

    private static func inspect(_ url: URL) -> LogIdentity? {
        let fd = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW)
        guard fd >= 0 else { return nil }
        defer { Darwin.close(fd) }
        var state = stat()
        guard fstat(fd, &state) == 0, state.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              state.st_nlink == 1, state.st_uid == getuid(), state.st_size >= 0 else { return nil }
        return LogIdentity(device: state.st_dev, inode: state.st_ino, offset: UInt64(state.st_size))
    }

    private static func readSuffix(_ url: URL, identity: LogIdentity) -> Data? {
        let fd = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW)
        guard fd >= 0 else { return nil }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        defer { try? handle.close() }
        var state = stat()
        guard fstat(fd, &state) == 0, state.st_dev == identity.device,
              state.st_ino == identity.inode, state.st_nlink == 1,
              state.st_uid == getuid(), state.st_size >= 0,
              UInt64(state.st_size) >= identity.offset,
              UInt64(state.st_size) - identity.offset <= UInt64(maxReportBytes) else { return nil }
        do {
            try handle.seek(toOffset: identity.offset)
            let expected = Int(UInt64(state.st_size) - identity.offset)
            let data = try handle.read(upToCount: expected) ?? Data()
            guard data.count == expected else { return nil }
            return data
        } catch { return nil }
    }
}
