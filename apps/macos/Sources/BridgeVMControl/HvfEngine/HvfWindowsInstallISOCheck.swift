import Foundation

/// Create looks inside the chosen file the way scripts/build-hvf-windows-scripted-source.sh does at
/// Start (an Arm installer has efi/boot/bootaa64.efi), so an x64 ISO or a file that isn't a disc image
/// is refused before it is copied and sealed into a new VM.
enum HvfWindowsInstallISOCheck {
    static let notDiskImage =
        "This file isn’t a Windows installer disc image. Choose the Windows 11 for Arm ISO from Microsoft."
    static let notArm64 = "This ISO isn’t Windows 11 for Arm. Choose the Arm64 ISO from Microsoft."

    /// The sentence to show, or nil for a Windows for Arm installer. It attaches the image, so call it
    /// off the main thread.
    static func problem(isoPath: String) -> String? {
        // Disk Arbitration wants a mount point the user owns: the user's temporary folder, as the script.
        let mountRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-iso-check-\(UUID().uuidString)", isDirectory: true)
        guard (try? FileManager.default.createDirectory(at: mountRoot, withIntermediateDirectories: true)) != nil else {
            return nil
        }
        defer { rmdir(mountRoot.path) }
        let attach = hdiutil(["attach", "-readonly", "-nobrowse", "-noverify", "-noautoopen",
                              "-mountrandom", mountRoot.path, "-plist", isoPath])
        let entities = attach.status == 0 ? plist(attach.output)?["system-entities"] as? [[String: Any]] : nil
        // A file already open in Finder makes that attach fail as busy: read the user's own mount instead.
        guard let mounts = entities.map({ $0.compactMap { $0["mount-point"] as? String } })
                ?? mountsOfOpenImage(isoPath) else { return notDiskImage }
        let isArm = mounts.contains(where: hasArmBootLoader)
        // Detach what this check attached (also an image that mounted nothing), never the user's own mount.
        if entities != nil, mounts.allSatisfy({ $0.contains("/\(mountRoot.lastPathComponent)/") }),
           let device = entities?.compactMap({ $0["dev-entry"] as? String }).min(by: { $0.count < $1.count }),
           hdiutil(["detach", device, "-quiet"]).status != 0 {
            _ = hdiutil(["detach", device, "-force", "-quiet"])
        }
        return isArm ? nil : notArm64
    }

    private static func mountsOfOpenImage(_ path: String) -> [String]? {
        var file = stat()
        guard stat(path, &file) == 0,
              let images = plist(hdiutil(["info", "-plist"]).output)?["images"] as? [[String: Any]] else { return nil }
        for image in images {
            var other = stat()
            guard let imagePath = image["image-path"] as? String, stat(imagePath, &other) == 0,
                  other.st_dev == file.st_dev, other.st_ino == file.st_ino else { continue }
            return (image["system-entities"] as? [[String: Any]] ?? []).compactMap { $0["mount-point"] as? String }
        }
        return nil
    }

    private static func plist(_ data: Data) -> [String: Any]? {
        try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any]
    }

    private static func hasArmBootLoader(_ mountPoint: String) -> Bool {
        var item = URL(fileURLWithPath: mountPoint, isDirectory: true)
        for name in ["efi", "boot", "bootaa64.efi"] {
            guard let next = (try? FileManager.default.contentsOfDirectory(at: item, includingPropertiesForKeys: nil))?
                .first(where: { $0.lastPathComponent.lowercased() == name }) else { return false }
            item = next
        }
        return true
    }

    private static func hdiutil(_ arguments: [String]) -> (status: Int32, output: Data) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/hdiutil")
        process.arguments = arguments
        let output = Pipe()
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return (-1, Data()) }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus, data)
    }
}
