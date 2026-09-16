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
        for name in [
            "display.ppm", "display.ppm.tmp", "display.fb", "display.fb.tmp",
            "display.fb.iosurface", "input.ctl", "run.log"
        ] {
            let url = evidenceDirectory.appendingPathComponent(name)
            if fileManager.fileExists(atPath: url.path) {
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
        } else {
            try Data().write(to: controlURL)
        }
        return manifest
    }
}
