import Foundation

/// Copy the native selector's current pair while it owns the source. Once the
/// helper exits successfully, all remaining work uses only the verified copy.
enum HvfMediaImport {
    /// `converting` sees the format and the percent converted when the disk is QCOW2 or VHDX.
    static func copy(
        disk: String, vars: String, toDisk: URL, toVars: URL,
        helper: URL, fileManager: FileManager = .default,
        converting: @escaping (HvfImportedDiskFormat, Int) -> Void = { _, _ in }
    ) throws {
        let workspace = try fileManager.url(
            for: .itemReplacementDirectory, in: .userDomainMask,
            appropriateFor: toDisk.deletingLastPathComponent(), create: true)
        defer { try? fileManager.removeItem(at: workspace) }
        try fileManager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: workspace.path)
        let members: [(member: URL, target: URL)]
        if let format = HvfImportedDiskFormat.detect(path: disk) {
            // A BridgeVM pair's disk is always RAW, so a QCOW2 or VHDX source has no managed
            // generation to select: the helper converts the chosen file and the vars are copied.
            converting(format, 0)
            let converted = workspace.appendingPathComponent("disk.raw")
            let copiedVars = workspace.appendingPathComponent("vars.fd")
            try HvfMediaImportHelper.invoke(helper, arguments: ["convert-to-raw", disk, converted.path]) {
                converting(format, $0)
            }
            try fileManager.copyItem(at: URL(fileURLWithPath: vars), to: copiedVars)
            members = [(converted, toDisk), (copiedVars, toVars)]
        } else {
            let snapshot = workspace.appendingPathComponent("selected.snapshot", isDirectory: true)
            try HvfMediaImportHelper.invoke(helper, arguments: ["create", disk, vars, snapshot.path, "import", String(UInt64.max)])
            try HvfMediaImportHelper.invoke(helper, arguments: ["verify", snapshot.path])
            members = [(snapshot.appendingPathComponent("disk.raw"), toDisk),
                       (snapshot.appendingPathComponent("vars.fd"), toVars)]
        }
        // A clone keeps the source's mode (read-only media blocked Start in T19 r3), so each member becomes
        // 0600 in the private workspace, never via the library path. Existing targets fail; the caller rolls back.
        for (member, target) in members {
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: member.path)
            try fileManager.moveItem(atPath: member.path, toPath: target.path)
        }
    }
}
