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
        if vars.isEmpty {
            let copiedDisk = workspace.appendingPathComponent("disk.raw")
            let seededVars = workspace.appendingPathComponent("vars.fd")
            if let format = HvfImportedDiskFormat.detect(path: disk) {
                converting(format, 0)
                try HvfMediaImportHelper.invoke(helper, arguments: ["convert-to-raw", disk, copiedDisk.path]) {
                    converting(format, $0)
                }
            } else {
                let selected = workspace.appendingPathComponent("selected.snapshot", isDirectory: true)
                try HvfMediaImportHelper.invoke(helper, arguments: ["import-disk", disk, selected.path])
                try fileManager.moveItem(at: selected.appendingPathComponent("disk.raw"), to: copiedDisk)
                let pairedVars = selected.appendingPathComponent("vars.fd")
                if fileManager.fileExists(atPath: pairedVars.path) {
                    try fileManager.moveItem(at: pairedVars, to: seededVars)
                }
            }
            // Managed disks bring their selected vars. Seed only an unmanaged private copy.
            if !fileManager.fileExists(atPath: seededVars.path) {
                try HvfWindowsBootSeed.writeBundledSeed(to: seededVars.path)
                do {
                    try HvfWindowsBootSeed.seedFile(varsPath: seededVars.path, diskPath: copiedDisk.path)
                } catch HvfWindowsBootSeed.SeedError.gptUnreadable {
                    try HvfWindowsImportNotice.write(nextTo: toVars)
                } catch HvfWindowsBootSeed.SeedError.espNotFound {
                    try HvfWindowsImportNotice.write(nextTo: toVars)
                }
            }
            members = [(copiedDisk, toDisk), (seededVars, toVars)]
        } else if let format = HvfImportedDiskFormat.detect(path: disk) {
            // A BridgeVM pair's disk is always RAW, so a QCOW2 or VHDX source has no managed
            // generation to select: the helper converts the chosen file and the vars are copied.
            converting(format, 0)
            let converted = workspace.appendingPathComponent("disk.raw")
            let copiedVars = workspace.appendingPathComponent("vars.fd")
            try HvfMediaImportHelper.invoke(helper, arguments: ["convert-to-raw", disk, converted.path]) {
                converting(format, $0)
            }
            // Resolved first: copyItem copies a symlink as a link, which would leave the bundle
            // sharing the source's vars.
            try fileManager.copyItem(at: URL(fileURLWithPath: vars).resolvingSymlinksInPath(), to: copiedVars)
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
