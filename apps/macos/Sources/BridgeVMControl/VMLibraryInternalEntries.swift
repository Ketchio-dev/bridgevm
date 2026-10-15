import Darwin
import Foundation

/// The install source cache is not a VM. A vm.json or uncertain lookup keeps an
/// entry visible to the ordinary safety diagnostics; aliases are never hidden.
enum VMLibraryInternalEntries {
    static func isSourceCache(_ directory: URL) -> Bool {
        guard directory.lastPathComponent == "Derived" else { return false }
        var info = stat()
        guard lstat(directory.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR else { return false }
        return lstat(directory.appendingPathComponent("vm.json").path, &info) != 0 && errno == ENOENT
    }
}
