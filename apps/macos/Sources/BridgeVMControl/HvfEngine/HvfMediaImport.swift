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
        // A clone keeps the source's mode (read-only media blocked Start in T19 r3), so each member becomes
        // 0600 in the private workspace, never via the library path. Existing targets fail; the caller rolls back.
        for (name, target) in [("disk.raw", toDisk), ("vars.fd", toVars)] {
            let member = snapshot.appendingPathComponent(name).path
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: member)
            try fileManager.moveItem(atPath: member, toPath: target.path)
        }
    }
}
