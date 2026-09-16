import Darwin
import Foundation

final class NativeRuntimeOwner: @unchecked Sendable {
    let library: NativeRuntimeLibraryHandle
    let endpoint: NativeRuntimeEndpoint
    private let descriptor: Int32
    private let lockIdentity: NativeRuntimeFileIdentity
    private let mutex = NSLock()
    private var closed = false
    private var server: NativeRuntimeServer?

    init(library: NativeRuntimeLibraryHandle) throws {
        try library.validateCurrentIdentity()
        let endpoint = try NativeRuntimeEndpoint(library: library.identity, create: true)
        try endpoint.validate()
        let fd = Darwin.open(endpoint.lockPath, O_RDWR | O_CREAT | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC, 0o600)
        guard fd >= 0 else { throw NativeRuntimeError.invalidEndpoint }
        do {
            var info = stat(), pathInfo = stat()
            guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
                  info.st_mode & 0o777 == 0o600, info.st_uid == geteuid(), info.st_nlink == 1,
                  lstat(endpoint.lockPath, &pathInfo) == 0,
                  NativeRuntimeFileIdentity(info) == NativeRuntimeFileIdentity(pathInfo)
            else { throw NativeRuntimeError.invalidEndpoint }
            guard flock(fd, LOCK_EX | LOCK_NB) == 0 else {
                throw errno == EWOULDBLOCK ? NativeRuntimeError.ownerBusy : .invalidEndpoint
            }
            try library.validateCurrentIdentity()
            try endpoint.validate()
            _ = try endpoint.socketIdentity()
            self.library = library; self.endpoint = endpoint; descriptor = fd
            lockIdentity = NativeRuntimeFileIdentity(info)
        } catch { Darwin.close(fd); throw error }
    }

    func start(controlHandler: NativeRuntimeRequestRouter.ControlHandler? = nil, startHandler: NativeRuntimeRequestRouter.StartHandler? = nil,
               handler: @escaping NativeRuntimeServer.Handler) throws {
        mutex.lock(); defer { mutex.unlock() }
        guard !closed, server == nil else { throw NativeRuntimeError.ownerBusy }
        try validateLease()
        server = try NativeRuntimeServer(library: library, endpoint: endpoint,
            validateOwner: { [weak self] in
                guard let self else { throw NativeRuntimeError.ownerUnavailable }
                try self.validateCurrentOwnership()
            }, controlHandler: controlHandler, startHandler: startHandler, handler: handler)
    }

    func validateCurrentOwnership() throws {
        mutex.lock(); defer { mutex.unlock() }
        guard !closed else { throw NativeRuntimeError.ownerUnavailable }
        try validateLease()
    }

    private func validateLease() throws {
        try NativeRuntimeOwnerLease.validate(library: library, endpoint: endpoint,
                                             descriptor: descriptor, identity: lockIdentity)
    }

    func close() {
        mutex.lock()
        guard !closed else { mutex.unlock(); return }
        closed = true
        let active = server; server = nil
        mutex.unlock()
        active?.close()
        // Never unlink the permanent lock inode: another owner may already be waiting.
        _ = flock(descriptor, LOCK_UN)
        Darwin.close(descriptor)
    }

    deinit { close() }
}
