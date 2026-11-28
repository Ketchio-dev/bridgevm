import Foundation

/// Copy the native selector's current pair while it owns the source. Once the
/// helper exits successfully, all remaining work uses only the verified copy.
enum HvfMediaImport {
    /// `converting` sees the format and the percent converted for a disk container.
    static func copy(
        disk: String, vars: String, toDisk: URL, toVars: URL,
        helper: URL, fileManager: FileManager = .default,
        available: (URL) -> Int64? = HvfImportSpace.available,
        converting: @escaping (HvfImportedDiskFormat, Int) -> Void = { _, _ in }
    ) throws {
        if HvfMediaSourceHolder.othersHaveOpen(disk) { throw HvfMediaSourceHolder.InUse() }
        if HvfImportSpace.tooLittle(disk: URL(fileURLWithPath: disk), into: toDisk.deletingLastPathComponent(),
                                    available: available) {
            throw CocoaError(.fileWriteOutOfSpace)
        }
        let workspace = try fileManager.url(
            for: .itemReplacementDirectory, in: .userDomainMask,
            appropriateFor: toDisk.deletingLastPathComponent(), create: true)
        defer { try? fileManager.removeItem(at: workspace) }
        try fileManager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: workspace.path)
        let copiedDisk = workspace.appendingPathComponent("disk.raw")
        let copiedVars = workspace.appendingPathComponent("vars.fd")
        func convert(_ format: HvfImportedDiskFormat, source: String) throws {
            converting(format, 0)
            try HvfMediaImportHelper.invoke(helper, arguments: ["convert-to-raw", source, copiedDisk.path]) {
                converting(format, $0)
            }
        }

        if let format = HvfImportedDiskFormat.detect(path: disk), format != .vhd {
            try convert(format, source: disk)
            if !vars.isEmpty {
                // copyItem would preserve a symlink into the original VM.
                try fileManager.copyItem(at: URL(fileURLWithPath: vars).resolvingSymlinksInPath(), to: copiedVars)
            }
        } else {
            // Older BridgeVM imports saved a whole fixed VHD as the VM's RAW disk.
            // A restore may have selected a newer disk and vars behind the original paths.
            let selected = workspace.appendingPathComponent("selected.snapshot", isDirectory: true)
            let arguments = vars.isEmpty ? ["import-disk", disk, selected.path]
                : ["create", disk, vars, selected.path, "import"]
            try HvfMediaImportHelper.invoke(helper, arguments: arguments)
            let selectedDisk = selected.appendingPathComponent("disk.raw")
            if let format = HvfImportedDiskFormat.detect(path: selectedDisk.path), format == .vhd || format == .nonFixedVHD {
                try convert(format, source: selectedDisk.path)
            } else {
                try fileManager.moveItem(at: selectedDisk, to: copiedDisk)
            }
            let pairedVars = selected.appendingPathComponent("vars.fd")
            if !vars.isEmpty || fileManager.fileExists(atPath: pairedVars.path) {
                try fileManager.moveItem(at: pairedVars, to: copiedVars)
            }
        }
        // Managed disks bring their selected vars. Seed only an unmanaged private copy.
        if vars.isEmpty && !fileManager.fileExists(atPath: copiedVars.path) {
            try HvfWindowsBootSeed.writeBundledSeed(to: copiedVars.path)
            do {
                try HvfWindowsBootSeed.seedFile(varsPath: copiedVars.path, diskPath: copiedDisk.path)
            } catch HvfWindowsBootSeed.SeedError.gptUnreadable {
                try HvfWindowsImportNotice.write(nextTo: toVars)
            } catch HvfWindowsBootSeed.SeedError.espNotFound {
                try HvfWindowsImportNotice.write(nextTo: toVars)
            }
        }
        // A cancel just after the last helper finished still leaves nothing behind.
        if HvfHelperProcess.cancelHandle?.isCancelled == true { throw HvfHelperProcess.Cancelled(signal: SIGTERM) }
        let members = [(copiedDisk, toDisk), (copiedVars, toVars)]
        // A clone keeps the source's mode (read-only media would block Start), so each member becomes
        // 0600 in the private workspace, never via the library path. Existing targets fail; the caller rolls back.
        for (member, target) in members {
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: member.path)
            try fileManager.moveItem(atPath: member.path, toPath: target.path)
        }
    }
}

/// Whether the drive that will hold the imported disk lacks the room for it, checked before the copy: it
/// used to run for as long as the copy took, fill the drive to the last byte (pausing VMs running from
/// it), and only then fail. The copy stays sparse, so it needs about the bytes the source holds, not
/// its size; a RAW disk cloned on its own APFS volume needs none.
enum HvfImportSpace {
    /// Left free after the copy, so the drive is not filled to the last byte.
    static let margin: Int64 = 1 << 30

    static func available(_ directory: URL) -> Int64? {
        let values = try? directory.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey,
                                                             .volumeAvailableCapacityKey])
        return values?.volumeAvailableCapacityForImportantUsage ?? values?.volumeAvailableCapacity.map(Int64.init)
    }

    static func tooLittle(disk: URL, into directory: URL, available: (URL) -> Int64?) -> Bool {
        let keys: Set<URLResourceKey> = [.totalFileAllocatedSizeKey, .volumeIdentifierKey]
        guard let source = try? disk.resolvingSymlinksInPath().resourceValues(forKeys: keys),
              let held = source.totalFileAllocatedSize, let free = available(directory) else { return false }
        let target = try? directory.resourceValues(forKeys: [.volumeIdentifierKey])
        let sameVolume = source.volumeIdentifier.map { $0.isEqual(target?.volumeIdentifier) } ?? false
        if sameVolume && HvfImportedDiskFormat.detect(path: disk.path) == nil { return false }
        return Int64(held) + margin > free
    }
}
