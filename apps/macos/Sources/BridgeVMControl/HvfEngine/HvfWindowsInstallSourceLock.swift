import Foundation
#if os(macOS)
import Darwin
#endif

final class HvfWindowsInstallSourceLock {
    enum LockError: LocalizedError, Equatable {
        case unsafePath, busy, unavailable

        var errorDescription: String? {
            switch self {
            case .unsafePath: return "The installation cache lock is at an unsafe location."
            case .busy: return "Another task is preparing the same Windows installation files. Wait for it to finish, then retry."
            case .unavailable: return "BridgeVM couldn’t lock the Windows installation cache."
            }
        }
    }

    private var descriptor: Int32 = -1

    init(sourceImagePath: String, fileManager: FileManager = .default) throws {
        let source = URL(fileURLWithPath: sourceImagePath).standardizedFileURL
        let parent = source.deletingLastPathComponent()
        try fileManager.createDirectory(at: parent, withIntermediateDirectories: true)
        let canonical: (String) -> String = { $0.replacingOccurrences(of: "/private/tmp/", with: "/tmp/", options: .anchored) }
        guard canonical(parent.path) == canonical(parent.resolvingSymlinksInPath().standardizedFileURL.path) else {
            throw LockError.unsafePath
        }
        let lock = URL(fileURLWithPath: source.path + ".lock")
        descriptor = open(lock.path, O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW, S_IRUSR | S_IWUSR)
        guard descriptor >= 0 else { throw LockError.unavailable }
        guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
            close(descriptor)
            descriptor = -1
            if errno == EWOULDBLOCK { throw LockError.busy }
            throw LockError.unavailable
        }
    }

    deinit {
        if descriptor >= 0 {
            _ = flock(descriptor, LOCK_UN)
            close(descriptor)
        }
    }
}
