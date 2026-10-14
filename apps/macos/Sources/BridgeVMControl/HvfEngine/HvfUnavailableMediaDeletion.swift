import Darwin
import Foundation

/// The ordinary lease opens the final media and the pending guard needs the bundle,
/// so an external registration whose bundle (or, once installed, both final media
/// paths) is gone can never be deleted through them. Only that case removes the
/// entry without a lease; no external path is modified. Absence is lstat/ENOENT,
/// never fileExists; other lookup errors refuse. Caller checks liveness/admission.
enum HvfUnavailableMediaDeletion {
    /// nil means this is not such a deletion: use the ordinary lease path.
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool? {
        let media = HvfMediaLeaseSession.finalMediaPaths(config)
        guard [config.bundlePath, media.disk, media.vars].allSatisfy({ $0.hasPrefix("/") }),
              isRegistrationOnly(config, rootURL: rootURL) else { return nil }
        do {
            guard try config.bundleIsAbsent()
                || (config.installPending != true && absent(media.disk) && absent(media.vars)) else { return nil }
        } catch { return false }
        // The saved path proves nothing about what the recursive delete removes: a
        // managed bundle behind a stale absolute path still lives in the entry.
        guard onlyRegistration(in: rootURL.appendingPathComponent(config.slug, isDirectory: true)) else { return false }
        return LibraryDeletionReferences.delete(config, rootURL: rootURL)
    }

    private static func onlyRegistration(in entry: URL) -> Bool {
        var info = stat()
        guard lstat(entry.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              let names = try? FileManager.default.contentsOfDirectory(atPath: entry.path),
              Set(names).subtracting([".DS_Store"]) == ["vm.json"],
              lstat(entry.appendingPathComponent("vm.json").path, &info) == 0, info.st_mode & S_IFMT == S_IFREG
        else { return false }
        return true
    }

    private static func isRegistrationOnly(_ config: VMConfig, rootURL: URL) -> Bool {
        let entry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        let bundle = URL(fileURLWithPath: config.bundlePath)
        return VMLibrary.deletionImpact(for: config, rootURL: rootURL) == .registrationOnly
            && !within(bundle.standardizedFileURL.path, entry.standardizedFileURL.path)
            && !within(resolved(bundle), entry.resolvingSymlinksInPath().standardizedFileURL.path)
            && !VMRelocationJournal.isPending(config, rootURL: rootURL)
            && (try? NativeLibraryReader.readConfig(rootURL: rootURL, id: config.slug)) == config
    }

    /// An absent path resolves through its deepest existing ancestor, so a managed
    /// bundle spelled through a symlinked library root is still inside the entry.
    private static func resolved(_ url: URL) -> String {
        var existing = url.standardizedFileURL
        var missing: [String] = []
        var info = stat()
        while existing.path != "/", lstat(existing.path, &info) != 0 {
            missing.insert(existing.lastPathComponent, at: 0)
            existing.deleteLastPathComponent()
        }
        return missing.reduce(existing.resolvingSymlinksInPath()) { $0.appendingPathComponent($1) }
            .standardizedFileURL.path
    }

    private static func within(_ path: String, _ directory: String) -> Bool {
        path == directory || path.hasPrefix(directory.hasSuffix("/") ? directory : directory + "/")
    }

    static func absent(_ path: String) throws -> Bool {
        var info = stat()
        if lstat(path, &info) == 0 { return false }
        guard errno == ENOENT else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
        return true
    }
}

extension HvfMediaLeaseSession {
    /// The exact paths the ordinary and pending-delete leases are taken on.
    static func finalMediaPaths(_ config: VMConfig) -> (disk: String, vars: String) {
        (config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw"), config.bundlePath + "/metadata/hvf-vars.fd")
    }
}
