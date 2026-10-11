import Darwin
import Foundation

/// The ordinary lease opens the final media, so an installed registration whose
/// external bundle (or both final media paths) is gone can never reach READY.
/// Only that case removes the library entry without the lease; no external path
/// is modified. Absence is lstat/ENOENT, never fileExists; any other lookup error
/// refuses. Liveness and admission are checked by the caller beforehand.
enum HvfUnavailableMediaDeletion {
    /// nil means this is not such a deletion: use the ordinary lease path.
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool? {
        let media = HvfMediaLeaseSession.finalMediaPaths(config)
        guard [config.bundlePath, media.disk, media.vars].allSatisfy({ $0.hasPrefix("/") }),
              isRegistrationOnly(config, rootURL: rootURL) else { return nil }
        do {
            guard try absent(config.bundlePath) || (absent(media.disk) && absent(media.vars)) else { return nil }
        } catch { return false }
        return VMLibrary.delete(config.slug, rootURL: rootURL)
    }

    private static func isRegistrationOnly(_ config: VMConfig, rootURL: URL) -> Bool {
        let entry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        let bundle = URL(fileURLWithPath: config.bundlePath)
        return config.installPending != true
            && VMLibrary.deletionImpact(for: config, rootURL: rootURL) == .registrationOnly
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

    private static func absent(_ path: String) throws -> Bool {
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
