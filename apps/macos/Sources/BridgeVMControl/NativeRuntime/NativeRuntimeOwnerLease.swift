import Darwin
import Foundation

enum NativeRuntimeOwnerLease {
    static func validate(library: NativeRuntimeLibraryHandle, endpoint: NativeRuntimeEndpoint,
                         descriptor: Int32, identity: NativeRuntimeFileIdentity) throws {
        try library.validateCurrentIdentity()
        try endpoint.validate()
        var held = stat(), path = stat()
        guard fstat(descriptor, &held) == 0, lstat(endpoint.lockPath, &path) == 0,
              path.st_mode & S_IFMT == S_IFREG, path.st_mode & 0o777 == 0o600,
              path.st_uid == geteuid(), path.st_nlink == 1,
              NativeRuntimeFileIdentity(held) == identity,
              NativeRuntimeFileIdentity(path) == identity else { throw NativeRuntimeError.invalidEndpoint }
    }
}
