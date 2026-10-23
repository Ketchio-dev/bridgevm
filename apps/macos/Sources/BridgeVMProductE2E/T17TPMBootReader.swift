import Foundation

/// Reads one boot's guest vTPM observation through the product launcher, as the other
/// T17 workloads are launched. It never throws: each failure is an unavailable record
/// with a fixed reason after a bounded wait, and the caller's journey goes on.
struct T17TPMBootReader {
    /// The launcher, the workload and its TBS helper must all reach the guest first.
    static let assets = ["bv-product-e2e-launch.ps1", "bv-product-e2e-tpm.ps1", "bv-product-e2e-tpm.cs"]
    let share: URL
    let runLog: URL
    let nonce: String
    var timeouts = T17TPMTimeouts()

    func read(_ boot: T17TPMBoot, enter: (String) throws -> Void) -> T17TPMBootRecord {
        var record = observe(boot, enter: enter)
        record.guestRestartBeforeRead = T17TPMRunLog.guestRestarted(runLog)
        return record
    }

    private func observe(_ boot: T17TPMBoot, enter: (String) throws -> Void) -> T17TPMBootRecord {
        guard poll(timeouts.asset, { T17TPMRunLog.synchronized(Self.assets, in: runLog) }) else {
            return .unavailable(boot, .assetMissing)
        }
        let action = "TpmBoot\(boot.rawValue)", prefix = String(nonce.prefix(12))
        let command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\bridgevm-share\\bv-product-e2e-launch.ps1 -Action \(action) -Nonce \(nonce)"
        let before = T17TPMRunLog.size(runLog)
        guard (try? enter(command)) != nil, poll(timeouts.launch, {
            T17TPMRunLog.replied(runLog, after: before, command: command, marker: "T17-LAUNCHED-\(action)-\(prefix)")
        }) else { return .unavailable(boot, .launchFailed) }
        let result = share.appendingPathComponent(T17TPMGuestResult.name(boot: boot, prefix: prefix))
        var outcome: Result<T17TPMBootRecord, T17TPMReason> = .failure(.resultMissing)
        // A malformed result is read again until the deadline: the share may still be copying it.
        _ = poll(timeouts.result) {
            outcome = T17TPMGuestResult.read(result, nonce: nonce, boot: boot)
            return (try? outcome.get()) != nil
        }
        switch outcome {
        case .success(let record): return record
        case .failure(let reason): return .unavailable(boot, reason)
        }
    }

    private func poll(_ timeout: TimeInterval, _ predicate: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if predicate() { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        return predicate()
    }
}

/// Bounded reads of the current boot's run log for the TPM read.
enum T17TPMRunLog {
    static let maximumBytes = 64 * 1024 * 1024

    static func size(_ url: URL) -> Int { regularSize(url) ?? 0 }

    /// The size of a regular, non-symlink file, read without a URL resource cache.
    static func regularSize(_ url: URL) -> Int? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              attributes[.type] as? FileAttributeType == .typeRegular else { return nil }
        return (attributes[.size] as? NSNumber)?.intValue
    }

    static func synchronized(_ names: [String], in url: URL) -> Bool {
        guard let data = data(url) else { return false }
        let text = String(decoding: data, as: UTF8.self)
        return names.allSatisfy { text.contains("BVAGENT SHARE host->guest \($0) bytes=") }
    }

    static func replied(_ url: URL, after offset: Int, command: String, marker: String) -> Bool {
        guard let data = data(url), data.count > offset else { return false }
        return T17ControlReply.succeeded(tail: String(decoding: data.suffix(from: offset), as: UTF8.self),
                                         command: command, marker: marker)
    }

    /// Whether the log holds a guest SYSTEM_RESET stop record, which in product runs is
    /// a helper respawn against the same swtpm. Nil when the log cannot be read whole.
    static func guestRestarted(_ url: URL) -> Bool? {
        guard let data = data(url) else { return nil }
        return String(decoding: data, as: UTF8.self).split(whereSeparator: \.isNewline)
            .contains { $0.hasPrefix(HvfStopLine.systemResetPrefix) }
    }

    private static func data(_ url: URL) -> Data? {
        guard let size = regularSize(url), size <= maximumBytes,
              let data = try? Data(contentsOf: url), data.count <= maximumBytes else { return nil }
        return data
    }
}
