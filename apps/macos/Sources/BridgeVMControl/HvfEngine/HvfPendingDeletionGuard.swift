import Darwin
import Foundation

/// A missing path is established by lstat/ENOENT, never by fileExists (which
/// follows dangling links and hides lookup failures). No recovery is performed.
enum HvfPendingDeletionGuard {
    static func validate(_ config: VMConfig, rootURL: URL) throws {
        guard config.engineKind == .hvfEngine, config.bootMode == "windows-hvf",
              config.installPending == true,
              try NativeLibraryReader.readConfig(rootURL: rootURL, id: config.slug) == config else {
            throw failure()
        }
        let bundle = URL(fileURLWithPath: config.bundlePath)
        let disks = bundle.appendingPathComponent("disks", isDirectory: true)
        let metadata = bundle.appendingPathComponent("metadata", isDirectory: true)
        let disk = disks.appendingPathComponent("hvf-target.raw")
        guard config.bundlePath == bundle.path, config.diskPath == disk.path else { throw failure() }
        for directory in [rootURL, rootURL.appendingPathComponent(config.slug), bundle, disks, metadata] {
            guard directory.path == directory.standardizedFileURL.path,
                  directory.path == directory.resolvingSymlinksInPath().standardizedFileURL.path,
                  ownedDirectory(directory) else { throw failure() }
        }
        try HvfPendingDeletionReferences.validate(config, rootURL: rootURL)
        _ = try HvfWindowsInstallRequestSnapshot.load(
            bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName))
        for absent in [disk, metadata.appendingPathComponent("hvf-vars.fd"),
                       HvfWindowsInstallStaging.root(bundle: bundle),
                       metadata.appendingPathComponent("hvf-install-finalization"),
                       bundle.appendingPathComponent(HvfWindowsInstallRequest.doneFileName),
                       metadata.appendingPathComponent("secure-boot-provisioning.json"),
                       metadata.appendingPathComponent("vtpm"),
                       VMRelocationJournal.url(config, rootURL: rootURL)] {
            guard try kind(absent) == nil else { throw failure() }
        }
        for parent in [disks, metadata] {
            let entries = try FileManager.default.contentsOfDirectory(atPath: parent.path)
            guard !entries.contains(where: {
                $0.hasPrefix(".bridgevm-pair-") || $0.hasPrefix(".bridgevm-init-")
            }) else { throw failure() }
        }
    }

    private static func ownedDirectory(_ url: URL) -> Bool {
        var info = stat()
        return lstat(url.path, &info) == 0 && info.st_mode & S_IFMT == S_IFDIR
            && info.st_uid == geteuid() && info.st_mode & 0o022 == 0
    }

    private static func kind(_ url: URL) throws -> mode_t? {
        var info = stat()
        if lstat(url.path, &info) == 0 { return info.st_mode & S_IFMT }
        guard errno == ENOENT else { throw failure() }
        return nil
    }

    private static func failure() -> NSError {
        NSError(domain: "BridgeVM.PendingDeletion", code: 1, userInfo: [NSLocalizedDescriptionKey:
            "Deletion requires an unchanged, never-started Windows HVF installation registration."])
    }
}
