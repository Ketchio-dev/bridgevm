import Foundation

/// Manages MANY VMs on disk: each VM is a `vm.json` under the library dir.
/// This is the multi-VM substrate — the app enumerates, creates, and deletes
/// VMs here, and each VMConfig points at its own bundle (disks + metadata).
enum VMLibrary {
    /// ~/Library/Application Support/BridgeVM/vms/<slug>/vm.json
    static var root: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("BridgeVM/vms", isDirectory: true)
    }

    static func ensureRoot() {
        try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    private static func vmJSON(_ slug: String) -> URL {
        root.appendingPathComponent(slug, isDirectory: true).appendingPathComponent("vm.json")
    }

    /// Enumerate all VMs in the library (sorted by display name).
    static func list() -> [VMConfig] {
        ensureRoot()
        guard let entries = try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil) else { return [] }
        var out: [VMConfig] = []
        for dir in entries where (try? dir.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true {
            let f = dir.appendingPathComponent("vm.json")
            if let data = try? Data(contentsOf: f), var cfg = try? JSONDecoder().decode(VMConfig.self, from: data) {
                if cfg.id == nil { cfg.id = dir.lastPathComponent }
                out.append(cfg)
            }
        }
        return out.sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
    }

    static func save(_ config: VMConfig) {
        ensureRoot()
        var cfg = config
        if cfg.id == nil { cfg.id = VMConfig.slugify(cfg.name) }
        let dir = root.appendingPathComponent(cfg.slug, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let enc = JSONEncoder(); enc.outputFormatting = [.prettyPrinted, .sortedKeys]
        if let data = try? enc.encode(cfg) {
            try? data.write(to: dir.appendingPathComponent("vm.json"))
        }
    }

    static func delete(_ slug: String) {
        let dir = root.appendingPathComponent(slug, isDirectory: true)
        try? FileManager.default.removeItem(at: dir)
    }

    /// One-time import of the legacy single ~/.bridgevm-control/config.json into
    /// the library, so the VM the user already runs shows up as the first entry.
    @discardableResult
    static func migrateLegacyIfNeeded() -> Bool {
        guard list().isEmpty, var legacy = VMConfig.loadLegacy() else { return false }
        if legacy.id == nil { legacy.id = VMConfig.slugify(legacy.name) }
        if legacy.bootMode == nil { legacy.bootMode = "direct-kernel" }
        save(legacy)
        return true
    }
}
