import Darwin
import Foundation

struct NativeRuntimeFileIdentity: Equatable, Sendable {
    let device: dev_t
    let inode: ino_t
    init(_ value: stat) { device = value.st_dev; inode = value.st_ino }
}

final class NativeRuntimeEndpoint: @unchecked Sendable {
    let directoryPath: String
    var socketPath: String { directoryPath + "/status.sock" }
    var lockPath: String { directoryPath + "/owner.lock" }
    private let parentPath: String
    private let parentDescriptor: Int32
    private let descriptor: Int32
    private let uid: uid_t

    init(library: NativeRuntimeLibraryIdentity, create: Bool) throws {
        uid = geteuid()
        guard library.uid == uid else { throw NativeRuntimeError.peerRejected }
        parentPath = "/private/tmp/bridgevm-app-\(uid)"
        directoryPath = parentPath + "/" + library.namespaceDigest.prefix(32)
        parentDescriptor = try Self.directory(parentPath, create: create, uid: uid)
        do { descriptor = try Self.directory(directoryPath, create: create, uid: uid) }
        catch { Darwin.close(parentDescriptor); throw error }
    }

    private static func directory(_ path: String, create: Bool, uid: uid_t) throws -> Int32 {
        if create, mkdir(path, 0o700) != 0, errno != EEXIST { throw NativeRuntimeError.invalidEndpoint }
        let fd = Darwin.open(path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard fd >= 0 else { throw errno == ENOENT ? NativeRuntimeError.ownerUnavailable : .invalidEndpoint }
        var value = stat()
        guard fstat(fd, &value) == 0, value.st_mode & S_IFMT == S_IFDIR,
              value.st_uid == uid, value.st_mode & 0o777 == 0o700 else {
            Darwin.close(fd); throw NativeRuntimeError.invalidEndpoint
        }
        return fd
    }

    func validate() throws {
        for (path, fd) in [(parentPath, parentDescriptor), (directoryPath, descriptor)] {
            var current = stat(), held = stat()
            guard lstat(path, &current) == 0, fstat(fd, &held) == 0,
                  current.st_mode & S_IFMT == S_IFDIR, current.st_uid == uid,
                  current.st_mode & 0o777 == 0o700,
                  NativeRuntimeFileIdentity(current) == NativeRuntimeFileIdentity(held) else {
                throw NativeRuntimeError.invalidEndpoint
            }
        }
    }

    func socketIdentity() throws -> NativeRuntimeFileIdentity? {
        try validate()
        var info = stat()
        if lstat(socketPath, &info) != 0 {
            if errno == ENOENT { return nil }
            throw NativeRuntimeError.invalidEndpoint
        }
        guard info.st_mode & S_IFMT == S_IFSOCK, info.st_uid == uid,
              info.st_mode & 0o777 == 0o600 else { throw NativeRuntimeError.invalidEndpoint }
        return NativeRuntimeFileIdentity(info)
    }

    func removeSocket(ifIdentity identity: NativeRuntimeFileIdentity) throws {
        guard try socketIdentity() == identity else { throw NativeRuntimeError.invalidEndpoint }
        guard unlink(socketPath) == 0 else { throw NativeRuntimeError.invalidEndpoint }
    }

    deinit { Darwin.close(descriptor); Darwin.close(parentDescriptor) }
}
