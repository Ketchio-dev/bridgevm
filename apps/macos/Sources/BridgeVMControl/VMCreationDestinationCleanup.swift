import Foundation
import Darwin

enum VMCreationDestinationCleanup {
    /// A competing creator can publish into the directory reserved by this
    /// attempt. Preserve every registration entry, including unreadable or
    /// malformed entries, rather than removing the other creator's result.
    @discardableResult
    static func removeIfUnpublished(_ root: URL, libraryRoot: URL? = nil, fileManager: FileManager = .default) -> Bool {
        let registration = libraryRoot?.appendingPathComponent(root.lastPathComponent) ?? root
        for directory in [root, registration] {
            var metadata = stat()
            guard lstat(directory.appendingPathComponent("vm.json").path, &metadata) != 0,
                  errno == ENOENT else { return false }
        }
        do {
            try fileManager.removeItem(at: root)
            return true
        } catch { return false }
    }
}
