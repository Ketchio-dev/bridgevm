import Darwin
import Foundation

enum HvfRuntimePreparation {
    @discardableResult
    static func prepare(config: HvfEngineConfig) throws -> Data {
        let fileManager = FileManager.default
        let evidenceDirectory = URL(fileURLWithPath: config.evidenceDir, isDirectory: true)
        try fileManager.createDirectory(
            at: evidenceDirectory,
            withIntermediateDirectories: true
        )
        // run.log is removed too: the wrapper recreates it, and a stale log
        // would otherwise replay old BVAGENT/BOOT_TIMER lines into this
        // session (false attach, false 3D-injection confirmation).
        // ramfb/ (the helper's boot screenshots, about 7 MB a frame, three or more
        // each boot) and the 3D command trace are written anew by every run and were
        // never removed, so a VM's bundle grew with each start. The last run's stay
        // until the next start, for diagnostics.
        for name in [
            "display.ppm", "display.ppm.tmp", "display.fb", "display.fb.tmp",
            "display.fb.iosurface", "display.cursor", "input.ctl", "run.log", "diagnostic-stop.request", "diagnostic-stop.request.pending",
            "ramfb", "virtio-gpu.jsonl"
        ] {
            let url = evidenceDirectory.appendingPathComponent(name)
            if fileManager.fileExists(atPath: url.path) || (try? fileManager.destinationOfSymbolicLink(atPath: url.path)) != nil {
                try fileManager.removeItem(at: url)
            }
        }
        try Data().write(to: evidenceDirectory.appendingPathComponent("input.ctl"))
        // Hash the same bytes sent to the runner, not a later regeneration of mutable config.
        let manifest = Data(config.launchManifestJSON().utf8)
        try manifest.write(to: evidenceDirectory.appendingPathComponent("launch-manifest.json"))

        let controlURL = URL(fileURLWithPath: config.ctlFilePath)
        try fileManager.createDirectory(
            at: controlURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        if fileManager.fileExists(atPath: controlURL.path) {
            let attributes = try fileManager.attributesOfItem(atPath: controlURL.path)
            guard attributes[.type] as? FileAttributeType == .typeRegular else {
                throw NSError(
                    domain: "BridgeVM.HvfEngineSession",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "control path is not a regular file: \(controlURL.path)"]
                )
            }
            // Every command the app sent the last run, typed text and keys included (base64),
            // would otherwise stay here and grow with each run. The new helper reads from the
            // end, so an empty file loses nothing.
            try clear(controlURL)
        } else {
            try Data().write(to: controlURL)
        }
        return manifest
    }

    /// Empties the file in place, without following a link that replaced it after the check.
    private static func clear(_ url: URL) throws {
        let descriptor = Darwin.open(url.path, O_WRONLY | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        defer { Darwin.close(descriptor) }
        guard ftruncate(descriptor, 0) == 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    }
}
