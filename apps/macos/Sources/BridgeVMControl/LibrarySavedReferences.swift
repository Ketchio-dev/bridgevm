import Darwin
import Foundation

/// Inspect saved registrations without recovery. The affected path can be an
/// entry being deleted or a bundle being moved; neither operation is a lock.
enum LibrarySavedReferences {
    static func validate(_ selected: VMConfig, rootURL: URL, affectedPath: URL) throws {
        let names = try FileManager.default.contentsOfDirectory(atPath: rootURL.path)
        guard names.count <= 10_000 else { throw failure() }
        for name in names where name != selected.slug {
            let entry = rootURL.appendingPathComponent(name)
            guard let entryKind = try kind(entry), entryKind != S_IFLNK else { throw failure() }
            guard entryKind == S_IFDIR else { continue }
            // Library caches without vm.json are not registrations.
            guard try kind(entry.appendingPathComponent("vm.json")) != nil else { continue }
            let config = try NativeLibraryReader.readConfig(rootURL: rootURL, id: name)
            guard !HvfPendingDeletionReferences.references(config: config, removedEntry: affectedPath) else { throw failure() }
            guard config.engineKind == .hvfEngine else { continue }
            let requestURL = URL(fileURLWithPath: config.bundlePath)
                .appendingPathComponent(HvfWindowsInstallRequest.fileName)
            if try kind(requestURL) == nil {
                guard try config.installPending != true || config.isDetachedRegistration(rootURL: rootURL) else { throw failure() }
                continue
            }
            let request = try HvfWindowsInstallRequestSnapshot.load(requestURL).request
            guard !HvfPendingDeletionReferences.references(config: config, request: request, removedEntry: affectedPath) else { throw failure() }
        }
    }

    private static func kind(_ url: URL) throws -> mode_t? {
        var info = stat()
        if lstat(url.path, &info) == 0 { return info.st_mode & S_IFMT }
        guard errno == ENOENT else { throw failure() }
        return nil
    }

    private static func failure() -> NSError {
        NSError(domain: "BridgeVM.PendingDeletion", code: 2, userInfo: [NSLocalizedDescriptionKey:
            "Cannot establish that other saved VMs in this library do not reference the affected path."])
    }
}
