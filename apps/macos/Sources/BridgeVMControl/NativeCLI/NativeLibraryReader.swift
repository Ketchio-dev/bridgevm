import Foundation
import Darwin

/// Inventory only: never invoke VMLibrary.scan, which may recover installation state.
enum NativeLibraryReader {
    static func isCanonicalID(_ id: String) -> Bool {
        !id.isEmpty && id == VMConfig.slugify(id) && !id.contains("/")
    }

    static func snapshot(rootURL: URL, id: String? = nil) throws -> NativeLibrarySnapshot {
        if let id, !isCanonicalID(id) { throw NativeCLIError.invalid("Invalid VM ID.") }
        let root = open(rootURL.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard root >= 0 else {
            if errno == ENOENT, id == nil {
                return NativeLibrarySnapshot(libraryPath: rootURL.path, records: [], issues: [])
            }
            throw NativeCLIError.unavailable("Cannot open native VM library: \(rootURL.path)")
        }
        defer { close(root) }
        let names = try id.map { [$0] } ?? FileManager.default.contentsOfDirectory(atPath: rootURL.path).sorted()
        var records: [NativeLibraryRecord] = []
        var issues: [NativeLibraryIssue] = []
        for name in names.prefix(10_000) {
            let path = rootURL.appendingPathComponent(name).path
            var entry = stat()
            guard fstatat(root, name, &entry, AT_SYMLINK_NOFOLLOW) == 0 else {
                if id != nil, errno == ENOENT {
                    throw NativeCLIError.unavailable("No native VM configuration found for ID '\(name)'.")
                }
                issues.append(issue("entry-unavailable", path, "Cannot inspect this library entry."))
                continue
            }
            let kind = entry.st_mode & S_IFMT
            if kind != S_IFDIR && kind != S_IFLNK || (id == nil && VMLibraryInternalEntries.isSourceCache(URL(fileURLWithPath: path))) { continue }
            guard kind == S_IFDIR, isCanonicalID(name) else {
                issues.append(issue("unsafe-entry", path, "Symlink or noncanonical VM directory was not loaded."))
                continue
            }
            do {
                let config = try NativeLibraryConfigLoader.readConfig(rootDescriptor: root, id: name)
                records.append(NativeLibraryRecord(config: config, rootURL: rootURL))
            } catch {
                issues.append(issue("config-unavailable", path + "/vm.json", error.localizedDescription))
            }
        }
        if names.count > 10_000 {
            issues.append(issue("inventory-limit", rootURL.path, "More than 10,000 entries; inventory is incomplete."))
        }
        if id != nil, records.isEmpty, issues.isEmpty {
            throw NativeCLIError.unavailable("No native VM configuration found for the requested ID.")
        }
        return NativeLibrarySnapshot(libraryPath: rootURL.path, records: records, issues: issues)
    }

    static func readConfig(rootURL: URL, id: String) throws -> VMConfig {
        guard isCanonicalID(id) else { throw NativeCLIError.invalid("Invalid VM ID.") }
        let root = open(rootURL.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard root >= 0 else { throw NativeCLIError.unavailable("Cannot open native VM library: \(rootURL.path)") }
        defer { close(root) }
        return try NativeLibraryConfigLoader.readConfig(rootDescriptor: root, id: id)
    }

    static func entryMayExist(_ url: URL) -> Bool {
        var metadata = stat()
        if lstat(url.path, &metadata) == 0 { return true }
        return errno != ENOENT && errno != ENOTDIR
    }

    private static func issue(_ code: String, _ path: String, _ message: String) -> NativeLibraryIssue {
        NativeLibraryIssue(code: code, path: path, message: message)
    }
}

struct NativeLibraryIssue: Codable, Equatable {
    let code: String
    let path: String
    let message: String
}

struct NativeLibraryRecord: Codable, Equatable {
    let id: String
    let displayName: String
    let backendKind: String
    let cpuCount: Int?
    let memoryMiB: Int?
    let installPending: Bool?
    let configPath: String
    let bundlePath: String
    let runtimeState: String
    let recoveryObservations: [String]

    init(config: VMConfig, rootURL: URL) {
        let directoryID = config.slug
        // Use a stable spelling for JSON consumers and retain the directory spelling for paths.
        id = directoryID.precomposedStringWithCanonicalMapping
        displayName = config.displayName
        backendKind = config.backendKind
        cpuCount = config.cpuCount
        memoryMiB = config.memMiB
        installPending = config.installPending
        configPath = rootURL.appendingPathComponent(directoryID).appendingPathComponent("vm.json").path
        bundlePath = config.bundlePath
        runtimeState = "unobserved"
        var observations: [String] = []
        if VMRelocationJournal.isPending(config, rootURL: rootURL) {
            observations.append("relocation-record-present-or-unreadable")
        }
        let journal = HvfWindowsInstallFinalizationPaths(
            libraryRoot: rootURL, bundle: URL(fileURLWithPath: config.bundlePath), slug: directoryID).journal
        if NativeLibraryReader.entryMayExist(journal) {
            observations.append("install-finalization-record-present-or-unreadable")
        }
        recoveryObservations = observations
    }
}

struct NativeLibrarySnapshot: Encodable {
    let schema = "bridgevm.app-library.v1"
    let libraryPath: String
    let records: [NativeLibraryRecord]
    let issues: [NativeLibraryIssue]
    var complete: Bool { issues.isEmpty }

    enum CodingKeys: String, CodingKey { case schema, libraryPath, records, issues, complete }
    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(schema, forKey: .schema)
        try container.encode(libraryPath, forKey: .libraryPath)
        try container.encode(records, forKey: .records)
        try container.encode(issues, forKey: .issues)
        try container.encode(complete, forKey: .complete)
    }
}

enum NativeLibraryConfigLoader {
    static func unknownID(_ id: String) -> String {
        "No VM has the ID ‘\(id)’ in this library. Run `bridgevm app list` to find its ID."
    }

    static func readConfig(rootDescriptor: Int32, id: String) throws -> VMConfig {
        guard NativeLibraryReader.isCanonicalID(id) else { throw NativeCLIError.invalid("Invalid VM ID.") }
        let directory = openat(rootDescriptor, id, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else {
            // A mistyped ID is the usual cause; say so, as start does.
            if errno == ENOENT { throw NativeCLIError.unavailable(unknownID(id)) }
            throw NativeCLIError.unavailable("Cannot open the VM directory safely.")
        }
        defer { close(directory) }
        let descriptor = openat(directory, "vm.json", O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { throw NativeCLIError.unavailable("Cannot open vm.json as a regular file.") }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var metadata = stat()
        let limit = VMLibrary.maximumConfigBytes
        guard fstat(descriptor, &metadata) == 0, metadata.st_mode & S_IFMT == S_IFREG,
              metadata.st_size >= 0, metadata.st_size <= limit else {
            throw NativeCLIError.unavailable("vm.json must be a regular file of at most \(limit) bytes.")
        }
        var data = Data()
        while data.count <= limit {
            guard let chunk = try handle.read(upToCount: min(65_536, limit + 1 - data.count)),
                  !chunk.isEmpty else { break }
            data.append(chunk)
        }
        guard data.count <= limit else { throw NativeCLIError.unavailable("vm.json exceeded the size limit while reading.") }
        var config = try JSONDecoder().decode(VMConfig.self, from: data)
        config.id = id
        return config
    }

}
