#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Darwin
import Foundation

final class AppUIDriverPrivateDirectory {
    let descriptor: Int32
    deinit { close(descriptor) }

    private init(descriptor: Int32) throws {
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              info.st_uid == geteuid(), info.st_mode & 0o7777 == 0o700 else {
            close(descriptor)
            throw AppUIDriverFailure.invalidFile
        }
        self.descriptor = descriptor
    }

    static func root(_ url: URL) throws -> AppUIDriverPrivateDirectory {
        let path = url.path
        guard url.isFileURL, AppUIDriverValidation.canonicalPath(path),
              url.lastPathComponent == "app-ui-private" else { throw AppUIDriverFailure.invalidFile }
        var current = open("/", O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard current >= 0 else { throw AppUIDriverFailure.ioFailure }
        for component in path.split(separator: "/") {
            let next = openat(current, String(component), O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            close(current)
            guard next >= 0 else { throw AppUIDriverFailure.invalidFile }
            current = next
        }
        return try AppUIDriverPrivateDirectory(descriptor: current)
    }

    func child(_ name: String) throws -> AppUIDriverPrivateDirectory {
        guard ["driver-requests", "driver-replies", "host-observations"].contains(name) else {
            throw AppUIDriverFailure.invalidFile
        }
        let child = openat(descriptor, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard child >= 0 else { throw AppUIDriverFailure.invalidFile }
        return try AppUIDriverPrivateDirectory(descriptor: child)
    }

    func createChildren() throws {
        guard mkdirat(descriptor, "driver-requests", 0o700) == 0 else { throw AppUIDriverFailure.fileExists }
        guard mkdirat(descriptor, "driver-replies", 0o700) == 0 else {
            _ = unlinkat(descriptor, "driver-requests", AT_REMOVEDIR)
            throw AppUIDriverFailure.fileExists
        }
        guard fsync(descriptor) == 0 else { throw AppUIDriverFailure.ioFailure }
    }
}
#endif
