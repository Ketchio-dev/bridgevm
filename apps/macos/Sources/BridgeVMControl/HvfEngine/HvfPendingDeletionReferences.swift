import Darwin
import Foundation

/// Only saved registrations in this library are inspected, without recovery.
/// This is a refusal check, not a cross-library or concurrent-writer lock.
enum HvfPendingDeletionReferences {
    static func validate(_ selected: VMConfig, rootURL: URL) throws {
        let removedEntry = rootURL.appendingPathComponent(selected.slug)
        let names = try FileManager.default.contentsOfDirectory(atPath: rootURL.path)
        guard names.count <= 10_000 else { throw failure() }
        for name in names where name != selected.slug {
            let entry = rootURL.appendingPathComponent(name)
            guard let entryKind = try kind(entry) else { throw failure() }
            // An aliased registration cannot be safely classified by this reader.
            guard entryKind != S_IFLNK else { throw failure() }
            guard entryKind == S_IFDIR else { continue }
            // Library caches (e.g. Derived) without vm.json are not registrations.
            guard try kind(entry.appendingPathComponent("vm.json")) != nil else { continue }
            let config = try NativeLibraryReader.readConfig(rootURL: rootURL, id: name)
            guard !references(config: config, removedEntry: removedEntry) else { throw failure() }
            guard config.engineKind == .hvfEngine else { continue }
            let requestURL = URL(fileURLWithPath: config.bundlePath)
                .appendingPathComponent(HvfWindowsInstallRequest.fileName)
            if try kind(requestURL) == nil {
                guard try config.installPending != true || config.isDetachedRegistration(rootURL: rootURL) else { throw failure() }
                continue
            }
            let request = try HvfWindowsInstallRequestSnapshot.load(requestURL).request
            guard !references(config: config, request: request, removedEntry: removedEntry) else { throw failure() }
        }
    }

    /// Also accepts an already-retained request; no filesystem records are loaded here.
    static func references(config: VMConfig, request: HvfWindowsInstallRequest? = nil,
                           removedEntry: URL) -> Bool {
        let entry = removedEntry.resolvingSymlinksInPath().standardizedFileURL.path
        let paths = [config.bundlePath, config.diskPath, config.isoPath, config.launchSpecPath,
                     config.runnerPath, config.handoffPath, config.sshKeyPath, config.leasesPath,
                     request?.isoPath, request?.driverPackageDir, request?.guestPayloadDirectory,
                     request?.guestPayloadManifest, request?.unattendedPath]
        for path in paths.compactMap({ $0 }) where !path.isEmpty {
            let url = URL(fileURLWithPath: path)
            // Both matter: deleting an in-entry symlink destroys a saved path even
            // when its target is external; an external alias can resolve inside.
            for candidate in [url.standardizedFileURL.path,
                              url.resolvingSymlinksInPath().standardizedFileURL.path] {
                if candidate == entry || candidate.hasPrefix(entry + "/") { return true }
            }
        }
        return false
    }

    private static func kind(_ url: URL) throws -> mode_t? {
        var info = stat()
        if lstat(url.path, &info) == 0 { return info.st_mode & S_IFMT }
        guard errno == ENOENT else { throw failure() }
        return nil
    }

    private static func failure() -> NSError {
        NSError(domain: "BridgeVM.PendingDeletion", code: 2, userInfo: [NSLocalizedDescriptionKey:
            "Deletion cannot establish that other saved VMs in this library do not reference the removed entry."])
    }
}
