import Darwin
import Foundation

/// A directory held by descriptor. Children are opened, created and removed
/// relative to it with O_NOFOLLOW, so a link at any name is refused or unlinked
/// itself, never followed.
final class HvfWindowsInstallPrivateDirectory {
    let url: URL
    private let descriptor: Int32
    private static let directoryFlags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC

    /// A private directory is owned by this user and not writable by anyone else,
    /// who could otherwise rename a child out from under the runner.
    private init(_ descriptor: Int32, url: URL, privateToOwner: Bool) throws {
        guard descriptor >= 0 else { throw Self.posixError(errno, url) }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              !privateToOwner || (info.st_uid == geteuid() && info.st_mode & 0o022 == 0) else {
            close(descriptor)
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
        self.descriptor = descriptor
        self.url = url
    }

    deinit { close(descriptor) }

    static func metadata(of bundle: URL) throws -> HvfWindowsInstallPrivateDirectory {
        try HvfWindowsInstallPrivateDirectory(open(bundle.path, directoryFlags), url: bundle, privateToOwner: true)
            .directory("metadata")
    }

    func directory(_ name: String, privateToOwner: Bool = true) throws -> HvfWindowsInstallPrivateDirectory {
        let child = url.appendingPathComponent(name, isDirectory: true)
        return try HvfWindowsInstallPrivateDirectory(
            openat(descriptor, name, Self.directoryFlags), url: child, privateToOwner: privateToOwner)
    }

    /// nil when absent. A link, or anything but a directory this user owns, is refused rather than replaced.
    func existingDirectory(_ name: String) throws -> HvfWindowsInstallPrivateDirectory? {
        var info = stat()
        guard fstatat(descriptor, name, &info, AT_SYMLINK_NOFOLLOW) == 0 else {
            if errno == ENOENT { return nil }
            throw failure(name)
        }
        guard info.st_mode & S_IFMT == S_IFDIR, info.st_uid == geteuid() else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.appendingPathComponent(name).path)
        }
        return try directory(name)
    }

    func makeDirectory(_ name: String) throws -> HvfWindowsInstallPrivateDirectory {
        guard mkdirat(descriptor, name, 0o700) == 0 else { throw failure(name) }
        let child = try directory(name)
        guard fchmod(child.descriptor, 0o700) == 0 else { throw Self.posixError(errno, child.url) }
        return child
    }

    /// An existing entry or link at `name` fails the call; the new file is 0600.
    func createFile(_ name: String, fill: (FileHandle) throws -> Void) throws {
        let file = openat(descriptor, name, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard file >= 0 else { throw failure(name) }
        let handle = FileHandle(fileDescriptor: file, closeOnDealloc: false)
        defer { try? handle.close() }
        try fill(handle)
    }

    /// Unlinks a non-directory entry; a link is removed itself, never its target.
    func unlink(_ name: String) throws {
        guard unlinkat(descriptor, name, 0) == 0 || errno == ENOENT else { throw failure(name) }
    }

    /// Removes an owned directory and everything below it, one descriptor-relative unlink at a time.
    func removeTree(_ name: String) throws {
        guard let child = try existingDirectory(name) else { return }
        try child.removeContents(depth: 1)
        guard unlinkat(descriptor, name, AT_REMOVEDIR) == 0 else { throw failure(name) }
    }

    private func removeContents(depth: Int) throws {
        guard depth <= 8 else { throw HvfWindowsInstallFinalizationError.unsafePath(url.path) }
        for name in try entryNames() {
            var info = stat()
            guard fstatat(descriptor, name, &info, AT_SYMLINK_NOFOLLOW) == 0 else { throw failure(name) }
            guard info.st_mode & S_IFMT == S_IFDIR else { try unlink(name); continue }
            try directory(name, privateToOwner: false).removeContents(depth: depth + 1)
            guard unlinkat(descriptor, name, AT_REMOVEDIR) == 0 else { throw failure(name) }
        }
    }

    private func entryNames() throws -> [String] {
        let copy = dup(descriptor)
        guard copy >= 0, let stream = fdopendir(copy) else {
            let code = errno
            if copy >= 0 { close(copy) }
            throw Self.posixError(code, url)
        }
        defer { closedir(stream) }
        rewinddir(stream)
        var names: [String] = []
        while let entry = readdir(stream) {
            let length = Int(entry.pointee.d_namlen)
            let name = withUnsafeBytes(of: entry.pointee.d_name) { String(decoding: $0.prefix(length), as: UTF8.self) }
            if name != ".", name != ".." { names.append(name) }
        }
        return names
    }

    private func failure(_ name: String) -> Error {
        let code = errno
        return Self.posixError(code, url.appendingPathComponent(name, isDirectory: false))
    }

    private static func posixError(_ code: Int32, _ url: URL) -> Error {
        NSError(domain: NSPOSIXErrorDomain, code: Int(code), userInfo: [NSFilePathErrorKey: url.path])
    }
}
