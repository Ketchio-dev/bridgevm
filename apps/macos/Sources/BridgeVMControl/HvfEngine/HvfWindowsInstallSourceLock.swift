import Foundation
#if os(macOS)
import Darwin
#endif

final class HvfWindowsInstallSourceLock {
    enum LockError: LocalizedError, Equatable {
        case unsafePath
        case busy
        case unavailable

        var errorDescription: String? {
            switch self {
            case .unsafePath: return "설치 소스 캐시 잠금 경로가 안전하지 않습니다."
            case .busy: return "같은 Windows 설치 소스를 다른 작업이 생성하고 있습니다. 잠시 후 다시 시도하세요."
            case .unavailable: return "설치 소스 캐시 잠금을 만들 수 없습니다."
            }
        }
    }

    private var descriptor: Int32 = -1

    init(sourceImagePath: String, fileManager: FileManager = .default) throws {
        let source = URL(fileURLWithPath: sourceImagePath).standardizedFileURL
        let parent = source.deletingLastPathComponent()
        try fileManager.createDirectory(at: parent, withIntermediateDirectories: true)
        guard parent.path == parent.resolvingSymlinksInPath().standardizedFileURL.path else {
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
