import Foundation
import Darwin

/// Owns one newly created registration directory; existing entries are never reused.
struct FirstRunImportDestination {
    struct NameInUse: Error {}

    let root: URL
    /// The requested ID, or the next free `<slug>-N` when only an unfinished import holds it.
    let slug: String
    private let device: dev_t
    private let inode: ino_t
    private let fileManager: FileManager

    init(slug: String, libraryRoot: URL, fileManager: FileManager) throws {
        guard slug == VMConfig.slugify(slug), slug.utf8.count <= VMLibrary.maximumVMSlugBytes else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        self.fileManager = fileManager
        try fileManager.createDirectory(at: libraryRoot, withIntermediateDirectories: true)
        var candidate = slug
        var suffix = 1
        // Foundation accepts an existing directory here. mkdir reserves it exclusively.
        while mkdir(libraryRoot.appendingPathComponent(candidate, isDirectory: true).path, S_IRWXU) != 0 {
            let code = POSIXErrorCode(rawValue: errno) ?? .EIO
            guard code == .EEXIST else { throw POSIXError(code) }
            // A folder with no vm.json is left by an import that was cut off (Force Quit, crash,
            // power loss). Keep its files and take the next free ID, as Create VM does.
            if suffix == 1, !Self.isUnfinished(libraryRoot.appendingPathComponent(slug, isDirectory: true)) {
                throw NameInUse()
            }
            suffix += 1
            candidate = "\(slug)-\(suffix)"
            guard candidate.utf8.count <= VMLibrary.maximumVMSlugBytes else { throw NameInUse() }
        }
        self.slug = candidate
        root = libraryRoot.appendingPathComponent(candidate, isDirectory: true)
        var metadata = stat()
        guard lstat(root.path, &metadata) == 0 else {
            let error = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
            _ = rmdir(root.path)
            throw error
        }
        device = metadata.st_dev
        inode = metadata.st_ino
    }

    /// A real folder (not a link) without a vm.json entry is not a saved VM.
    private static func isUnfinished(_ entry: URL) -> Bool {
        var metadata = stat()
        guard lstat(entry.path, &metadata) == 0, metadata.st_mode & S_IFMT == S_IFDIR else { return false }
        return lstat(entry.appendingPathComponent("vm.json").path, &metadata) != 0 && errno == ENOENT
    }

    @discardableResult func removeIfOwned() -> Bool {
        var metadata = stat()
        guard lstat(root.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFDIR,
              metadata.st_dev == device, metadata.st_ino == inode else { return false }
        return VMCreationDestinationCleanup.removeIfUnpublished(root, fileManager: fileManager)
    }
}
