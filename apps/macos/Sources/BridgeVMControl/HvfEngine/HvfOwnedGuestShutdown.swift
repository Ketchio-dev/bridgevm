import Foundation
import Darwin

/// The launch captures the existing file identity. All command I/O runs off MainActor.
final class HvfOwnedGuestShutdown: @unchecked Sendable {
    private let path: String
    private let expected: stat?
    private let queue: DispatchQueue
    private var descriptor: Int32 = -1
    private var attempted = false
    private let closeLock = NSLock()
    private var closing = false

    init(path: String, queue: DispatchQueue = DispatchQueue(label: "com.bridgevm.owned-guest-shutdown")) {
        self.path = path; self.queue = queue
        var value = stat()
        expected = lstat(path, &value) == 0 && Self.regularOwned(value) ? value : nil
        queue.async { [self] in
            guard !isClosing, let expected else { return }
            let fd = open(path, O_WRONLY | O_APPEND | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
            guard fd >= 0 else { return }
            var actual = stat()
            guard fstat(fd, &actual) == 0, Self.same(expected, actual), Self.regularOwned(actual) else {
                Darwin.close(fd); return
            }
            descriptor = fd
        }
    }

    func request(_ completion: @escaping @Sendable (Bool) -> Void) {
        queue.async { [self] in
            guard !isClosing, !attempted else { completion(false); return }
            attempted = true
            guard descriptor >= 0, let expected else { completion(false); return }
            var current = stat(), opened = stat()
            guard lstat(path, &current) == 0, fstat(descriptor, &opened) == 0,
                  Self.same(expected, current), Self.same(expected, opened),
                  Self.regularOwned(current), Self.regularOwned(opened) else { completion(false); return }
            guard !isClosing else { completion(false); return }
            let bytes = Array("shutdown.exe /p /f\n".utf8)
            // One short append: a partial command is not retried or reported as delivered.
            let written = bytes.withUnsafeBytes { Darwin.write(descriptor, $0.baseAddress, $0.count) }
            completion(written == bytes.count)
        }
    }

    private var isClosing: Bool {
        closeLock.lock(); defer { closeLock.unlock() }; return closing
    }

    func close() {
        closeLock.lock(); closing = true; closeLock.unlock()
        queue.async { [self] in
            if descriptor >= 0 { Darwin.close(descriptor); descriptor = -1 }
        }
    }

    private static func same(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
    }
    private static func regularOwned(_ value: stat) -> Bool {
        value.st_mode & S_IFMT == S_IFREG && value.st_uid == geteuid() && value.st_nlink == 1
    }
    deinit { if descriptor >= 0 { Darwin.close(descriptor) } }
}
