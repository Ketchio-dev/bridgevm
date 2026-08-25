import Darwin
import Foundation

enum HvfPrivateSnapshotPath {
    static func entryExists(_ url: URL) -> Bool {
        var status = stat()
        return lstat(url.path, &status) == 0 || errno != ENOENT
    }

    static func isPrivateParent(_ url: URL) -> Bool {
        guard url.isFileURL, url.path.hasPrefix("/") else { return false }
        let path = canonicalSystemTemporaryAlias(url.path)
        var current = "/"
        var finalStatus = stat()
        for component in (path as NSString).pathComponents.dropFirst() {
            current = (current as NSString).appendingPathComponent(component)
            var status = stat()
            guard lstat(current, &status) == 0,
                  status.st_mode & S_IFMT == S_IFDIR else { return false }
            finalStatus = status
        }
        return finalStatus.st_uid == geteuid() && finalStatus.st_mode & 0o077 == 0
    }

    private static func canonicalSystemTemporaryAlias(_ path: String) -> String {
        guard path == "/var" || path.hasPrefix("/var/") else { return path }
        var status = stat()
        var target = [CChar](repeating: 0, count: Int(PATH_MAX))
        let count = readlink("/var", &target, target.count)
        guard lstat("/var", &status) == 0, status.st_uid == 0,
              status.st_mode & S_IFMT == S_IFLNK, count == 11,
              String(cString: target) == "private/var" else { return path }
        return "/private" + path
    }
}
