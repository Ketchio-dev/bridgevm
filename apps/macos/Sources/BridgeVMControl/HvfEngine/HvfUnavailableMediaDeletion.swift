import Darwin
import Foundation

/// The ordinary lease opens the final media and the pending guard needs the bundle,
/// so a VM whose bundle (or, once installed, both final media paths) is gone can
/// never be deleted through them. Those cases remove the entry without a lease: an
/// external registration's entry holding only vm.json (no external path is
/// modified), or an installed VM's entry holding nothing but BridgeVM's records.
/// Absence is lstat/ENOENT, never fileExists; other lookup errors refuse. Caller
/// checks liveness/admission.
enum HvfUnavailableMediaDeletion {
    /// nil means this is not such a deletion: use the ordinary lease path.
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool? {
        let media = HvfMediaLeaseSession.finalMediaPaths(config)
        guard [config.bundlePath, media.disk, media.vars].allSatisfy({ $0.hasPrefix("/") }) else { return nil }
        guard isRegistrationOnly(config, rootURL: rootURL) else { return deleteWithoutMedia(config, rootURL: rootURL) }
        do {
            guard try config.bundleIsAbsent()
                || (config.installPending != true && absent(media.disk) && absent(media.vars)) else { return nil }
        } catch { return false }
        // The saved path proves nothing about what the recursive delete removes: a
        // managed bundle behind a stale absolute path still lives in the entry.
        guard onlyRegistration(in: rootURL.appendingPathComponent(config.slug, isDirectory: true)) else { return false }
        return LibraryDeletionReferences.delete(config, rootURL: rootURL)
    }

    /// An installed VM in the library whose disk and vars were both removed (in Finder, say): the
    /// lease cannot open them, so “could not be removed from disk” came back every time.
    private static func deleteWithoutMedia(_ config: VMConfig, rootURL: URL) -> Bool? {
        let media = HvfMediaLeaseSession.finalMediaPaths(config)
        guard config.installPending != true, !VMRelocationJournal.isPending(config, rootURL: rootURL),
              (try? NativeLibraryReader.readConfig(rootURL: rootURL, id: config.slug)) == config else { return nil }
        do {
            guard try absent(media.disk) && absent(media.vars) else { return nil }
        } catch { return false }
        let entry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        guard holdsOnlyRecords(entry, bundle: VMLibrary.deletionTarget(for: config, rootURL: rootURL)) else { return false }
        return LibraryDeletionReferences.delete(config, rootURL: rootURL)
    }

    /// Everything the recursive delete would remove is vm.json or BridgeVM's own record of a VM
    /// whose disk and vars are gone: logs, the installer copy, vTPM state, the driver copy and the
    /// files directly in metadata. A snapshot, a restored disk (disks/.bridgevm-pair-*), install
    /// staging or any other file keeps the entry. A link is judged by its own path, never followed.
    private static func holdsOnlyRecords(_ entry: URL, bundle: URL?) -> Bool {
        let entryPath = entry.standardizedFileURL.path + "/"
        let bundlePath = bundle?.standardizedFileURL.path
        let bundlePrefix = bundlePath.flatMap { $0.hasPrefix(entryPath) ? String($0.dropFirst(entryPath.count)) + "/" : nil }
        guard let paths = try? FileManager.default.subpathsOfDirectory(atPath: entry.path) else { return false }
        return paths.allSatisfy { path in
            var info = stat()
            guard lstat(entry.appendingPathComponent(path).path, &info) == 0 else { return false }
            if info.st_mode & S_IFMT == S_IFDIR || path == "vm.json" || (path as NSString).lastPathComponent == ".DS_Store" {
                return true
            }
            guard let bundlePrefix, path.hasPrefix(bundlePrefix) else { return false }
            let parts = path.dropFirst(bundlePrefix.count).split(separator: "/").map(String.init)
            switch parts.first {
            case "logs": return true
            case "disks": return parts == ["disks", "installer.iso"]
            case "metadata":
                return parts.count == 2 || (parts.count > 2 && ["vtpm", "vtpm-lifecycle", "windows-guest-payload"].contains(parts[1]))
            default: return false
            }
        }
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
