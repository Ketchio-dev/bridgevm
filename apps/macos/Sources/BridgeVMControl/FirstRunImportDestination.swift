import Foundation
import Darwin

/// Owns one newly created registration directory; existing entries are never reused.
struct FirstRunImportDestination {
    let root: URL
    private let device: dev_t
    private let inode: ino_t
    private let fileManager: FileManager

    init(slug: String, libraryRoot: URL, fileManager: FileManager) throws {
        guard slug == VMConfig.slugify(slug), slug.utf8.count <= VMLibrary.maximumVMSlugBytes else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        self.fileManager = fileManager
        root = libraryRoot.appendingPathComponent(slug, isDirectory: true)
        try fileManager.createDirectory(at: libraryRoot, withIntermediateDirectories: true)
        // Foundation accepts an existing directory here. mkdir reserves it exclusively.
        guard mkdir(root.path, S_IRWXU) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        var metadata = stat()
        guard lstat(root.path, &metadata) == 0 else {
            let error = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
            _ = rmdir(root.path)
            throw error
        }
        device = metadata.st_dev
        inode = metadata.st_ino
    }

    func removeIfOwned() {
        var metadata = stat()
        guard lstat(root.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFDIR,
              metadata.st_dev == device, metadata.st_ino == inode else { return }
        try? fileManager.removeItem(at: root)
    }

    func copyIndependent(from sourcePath: String, to destination: URL) throws {
        let source = URL(fileURLWithPath: sourcePath).resolvingSymlinksInPath()
        try fileManager.copyItem(atPath: source.path, toPath: destination.path)
    }
}
