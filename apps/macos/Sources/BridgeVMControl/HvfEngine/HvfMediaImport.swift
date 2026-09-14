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
        try HvfMediaImportHelper.invoke(helper, arguments: [
            "create", disk, vars, snapshot.path, "import", String(UInt64.max)
        ])
        try HvfMediaImportHelper.invoke(helper, arguments: ["verify", snapshot.path])
        // Foundation refuses existing destinations. The caller owns and rolls
        // back its newly reserved VM directory if either move fails.
        try fileManager.moveItem(atPath: snapshot.appendingPathComponent("disk.raw").path,
                                 toPath: toDisk.path)
        try fileManager.moveItem(atPath: snapshot.appendingPathComponent("vars.fd").path,
                                 toPath: toVars.path)
    }
}
