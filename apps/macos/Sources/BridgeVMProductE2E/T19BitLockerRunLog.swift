import Foundation

/// Bounded reads of the check's run log: the agent's READY, the guest's SYSTEM_RESET stop record,
/// the respawned helper's TPM power-cycle line and the final report's SYSTEM_OFF, each after an offset.
enum T19BitLockerRunLog {
    /// generation_tpm.rs prints this once per helper generation before its first guest instruction.
    static let powerCycle = "TPM2 TIS backend: power-cycled with CMD_INIT before boot"

    static func ready(_ url: URL, after offset: Int) -> Bool { lineEnd(url, after: offset) { A9FirstReadyLine.first(in: [$0]) != nil } != nil }

    /// The offset just past the first SYSTEM_RESET stop record after `offset`.
    static func reset(_ url: URL, after offset: Int) -> Int? { lineEnd(url, after: offset) { $0.hasPrefix(HvfStopLine.systemResetPrefix) } }

    /// Nil when the log cannot be read whole.
    static func powerCycled(_ url: URL, after offset: Int) -> Bool? {
        guard data(url) != nil else { return nil }
        return lineEnd(url, after: offset) { $0 == powerCycle } != nil
    }

    /// Only the final report's host-framed stop record counts (HvfTerminalReport), as for the journey's shutdowns.
    static func systemOff(_ url: URL, after offset: Int) -> Bool {
        guard let data = data(url), let stop = HvfTerminalReport.stop(in: data) else { return false }
        return stop.offset >= offset && stop.line == HvfStopLine.systemOff
    }

    /// Whether each name was pushed to the guest after `offset`. Every helper generation pushes the whole
    /// share again, truncating each guest copy first, so a launch waits for the generation's own pushes.
    static func synchronized(_ names: [String], in url: URL, after offset: Int) -> Bool {
        guard let data = data(url) else { return false }
        return names.allSatisfy { name in lineEnd(data, after: offset) { $0.hasPrefix("BVAGENT SHARE host->guest \(name) bytes=") } != nil }
    }

    /// The offset just past the first whole line after `offset` that `matches`, a trailing CR removed.
    static func lineEnd(_ url: URL, after offset: Int, _ matches: (String) -> Bool) -> Int? {
        data(url).flatMap { lineEnd($0, after: offset, matches) }
    }

    static func lineEnd(_ data: Data, after offset: Int, _ matches: (String) -> Bool) -> Int? {
        guard offset >= 0, data.count > offset else { return nil }
        var start = offset
        while let newline = data[start...].firstIndex(of: 0x0a) {
            var line = String(decoding: data[start..<newline], as: UTF8.self)
            if line.hasSuffix("\r") { line.removeLast() }
            if matches(line) { return newline + 1 }
            start = newline + 1
        }
        return nil
    }

    static func data(_ url: URL) -> Data? {
        guard let size = T17TPMRunLog.regularSize(url), size <= T17TPMRunLog.maximumBytes,
              let data = try? Data(contentsOf: url), data.count <= T17TPMRunLog.maximumBytes else { return nil }
        return data
    }
}

/// Copies the BitLocker launcher and workload from the packaged app into the lane share. Best effort,
/// like T17TPMGuestAssets: a missing, unsafe or oversized asset only makes the check `asset-missing`.
enum T19BitLockerGuestAssets {
    static let names = ["bv-product-e2e-bitlocker-launch.ps1", "bv-product-e2e-bitlocker.ps1"]

    static func stage(appBundlePath: String, sharePath: String, fileManager: FileManager = .default) {
        let assets = URL(fileURLWithPath: appBundlePath).appendingPathComponent("Contents/Resources/scripts/win-assets")
        for name in names {
            let source = assets.appendingPathComponent(name)
            guard let attributes = try? fileManager.attributesOfItem(atPath: source.path),
                  attributes[.type] as? FileAttributeType == .typeRegular,
                  let size = (attributes[.size] as? NSNumber)?.intValue, size > 0, size <= T17TPMGuestAssets.maximumBytes else { continue }
            try? fileManager.copyItem(at: source, to: URL(fileURLWithPath: sharePath).appendingPathComponent(name))
        }
    }
}
