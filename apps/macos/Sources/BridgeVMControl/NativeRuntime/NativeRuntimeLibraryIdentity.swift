import CryptoKit
import Darwin
import Foundation

final class NativeRuntimeLibraryHandle: @unchecked Sendable {
    let identity: NativeRuntimeLibraryIdentity
    let descriptor: Int32

    private init(identity: NativeRuntimeLibraryIdentity, descriptor: Int32) {
        self.identity = identity; self.descriptor = descriptor
    }
    static func open(rootURL: URL, create: Bool) throws -> NativeRuntimeLibraryHandle {
        guard rootURL.isFileURL, rootURL.path.hasPrefix("/") else { throw NativeRuntimeError.libraryChanged }
        let canonical = rootURL.standardizedFileURL.resolvingSymlinksInPath()
        if create {
            try FileManager.default.createDirectory(at: canonical, withIntermediateDirectories: true,
                                                    attributes: [.posixPermissions: 0o700])
        }
        let fd = Darwin.open(canonical.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard fd >= 0 else { throw NativeRuntimeError.ownerUnavailable }
        do {
            var info = stat()
            guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
                  info.st_uid == geteuid() else { throw NativeRuntimeError.libraryChanged }
            var path = [CChar](repeating: 0, count: Int(MAXPATHLEN))
            guard fcntl(fd, F_GETPATH, &path) == 0 else { throw NativeRuntimeError.libraryChanged }
            guard let physical = realpath(String(cString: path), nil) else { throw NativeRuntimeError.libraryChanged }
            defer { free(physical) }
            let identity = NativeRuntimeLibraryIdentity(canonicalPath: String(cString: physical),
                device: UInt64(UInt32(bitPattern: info.st_dev)), inode: UInt64(info.st_ino), uid: info.st_uid)
            try validate(identity, descriptor: fd)
            return NativeRuntimeLibraryHandle(identity: identity, descriptor: fd)
        } catch { Darwin.close(fd); throw error }
    }

    func validateCurrentIdentity() throws {
        try Self.validate(identity, descriptor: descriptor)
    }

    private static func validate(_ identity: NativeRuntimeLibraryIdentity, descriptor: Int32) throws {
        var current = stat(), retained = stat()
        guard fstat(descriptor, &retained) == 0,
              lstat(identity.canonicalPath, &current) == 0,
              current.st_mode & S_IFMT == S_IFDIR, current.st_uid == identity.uid,
              current.st_dev == retained.st_dev, current.st_ino == retained.st_ino,
              UInt64(UInt32(bitPattern: current.st_dev)) == identity.device,
              UInt64(current.st_ino) == identity.inode else { throw NativeRuntimeError.libraryChanged }
    }

    deinit { Darwin.close(descriptor) }
}

extension NativeRuntimeLibraryIdentity {
    var namespaceDigest: String {
        let data = Data("bridgevm-native-runtime-v1\n\(uid)\n\(device)\n\(inode)\n".utf8)
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
