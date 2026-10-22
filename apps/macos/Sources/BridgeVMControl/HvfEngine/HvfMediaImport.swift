import Foundation

/// Copy the native selector's current pair while it owns the source. Once the
/// helper exits successfully, all remaining work uses only the verified copy.
enum HvfMediaImport {
    static func copy(
        disk: String, vars: String, toDisk: URL, toVars: URL,
        helper: URL, fileManager: FileManager = .default
    ) throws {
        let workspace = try fileManager.url(
            for: .itemReplacementDirectory, in: .userDomainMask,
            appropriateFor: toDisk.deletingLastPathComponent(), create: true)
        defer { try? fileManager.removeItem(at: workspace) }
        try fileManager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: workspace.path)
        let snapshot = workspace.appendingPathComponent("selected.snapshot", isDirectory: true)
        try HvfMediaImportHelper.invoke(helper, arguments: ["create", disk, vars, snapshot.path, "import", String(UInt64.max)])
        try HvfMediaImportHelper.invoke(helper, arguments: ["verify", snapshot.path])
        // Foundation refuses existing destinations; the caller rolls back its reserved VM directory
        // if a move fails. An APFS clone keeps the source's mode, and a read-only selection left the
        // VM's own disk and vars unwritable, so launch readiness refused them (T19 r3, 2026-10-22).
        for (name, target) in [("disk.raw", toDisk), ("vars.fd", toVars)] {
            try fileManager.moveItem(atPath: snapshot.appendingPathComponent(name).path, toPath: target.path)
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: target.path)
        }
    }
}
