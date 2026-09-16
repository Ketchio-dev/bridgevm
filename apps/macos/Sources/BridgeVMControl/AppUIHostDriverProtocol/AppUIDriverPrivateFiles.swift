#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Darwin
import Foundation

extension AppUIDriverPrivateDirectory {
    func read(_ name: String) throws -> Data? {
        try Self.validateName(name)
        let file = openat(descriptor, name, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard file >= 0 else {
            if errno == ENOENT { return nil }
            throw AppUIDriverFailure.invalidFile
        }
        defer { close(file) }
        var before = stat()
        guard fstat(file, &before) == 0, Self.validFile(before), before.st_size > 0,
              before.st_size <= AppUIDriverConstants.maximumBytes else { throw AppUIDriverFailure.invalidFile }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: AppUIDriverConstants.maximumBytes + 1)
        while true {
            let count = Darwin.read(file, &buffer, buffer.count)
            if count < 0 {
                if errno == EINTR { continue }
                throw AppUIDriverFailure.ioFailure
            }
            if count == 0 { break }
            guard data.count + count <= AppUIDriverConstants.maximumBytes else { throw AppUIDriverFailure.protocolLimit }
            data.append(contentsOf: buffer.prefix(count))
        }
        var after = stat()
        guard fstat(file, &after) == 0, Self.validFile(after), before.st_size == after.st_size,
              data.count == after.st_size, before.st_dev == after.st_dev, before.st_ino == after.st_ino,
              before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec,
              before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec,
              before.st_ctimespec.tv_sec == after.st_ctimespec.tv_sec,
              before.st_ctimespec.tv_nsec == after.st_ctimespec.tv_nsec else {
            throw AppUIDriverFailure.invalidFile
        }
        return data
    }

    func publish(_ data: Data, name: String) throws {
        try Self.validateName(name)
        guard !data.isEmpty, data.count <= AppUIDriverConstants.maximumBytes else { throw AppUIDriverFailure.protocolLimit }
        let staging = "." + name + ".staging"
        let file = openat(descriptor, staging, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard file >= 0 else { throw AppUIDriverFailure.fileExists }
        var openFile = true
        defer {
            if openFile { close(file) }
            _ = unlinkat(descriptor, staging, 0)
        }
        guard fchmod(file, 0o600) == 0 else { throw AppUIDriverFailure.ioFailure }
        try data.withUnsafeBytes { bytes in
            guard let start = bytes.baseAddress else { throw AppUIDriverFailure.invalidFile }
            var offset = 0
            while offset < bytes.count {
                let count = Darwin.write(file, start.advanced(by: offset), bytes.count - offset)
                if count < 0 && errno == EINTR { continue }
                guard count > 0 else { throw AppUIDriverFailure.ioFailure }
                offset += count
            }
        }
        guard fsync(file) == 0 else { throw AppUIDriverFailure.ioFailure }
        let closed = close(file); openFile = false
        guard closed == 0 else { throw AppUIDriverFailure.ioFailure }
        guard renameatx_np(descriptor, staging, descriptor, name, UInt32(RENAME_EXCL)) == 0 else {
            throw errno == EEXIST ? AppUIDriverFailure.fileExists : AppUIDriverFailure.ioFailure
        }
        guard fsync(descriptor) == 0 else { throw AppUIDriverFailure.ioFailure }
    }

    func cancellationExists() throws -> Bool {
        var info = stat()
        if fstatat(descriptor, "cancel.requested", &info, AT_SYMLINK_NOFOLLOW) == 0 {
            guard Self.validFile(info), info.st_size <= AppUIDriverConstants.maximumBytes else {
                throw AppUIDriverFailure.invalidFile
            }
            return true
        }
        if errno == ENOENT { return false }
        throw AppUIDriverFailure.ioFailure
    }

    private static func validFile(_ info: stat) -> Bool {
        info.st_mode & S_IFMT == S_IFREG && info.st_uid == geteuid()
            && info.st_mode & 0o7777 == 0o600 && info.st_nlink == 1
    }

    private static func validateName(_ name: String) throws {
        guard !name.isEmpty, name.utf8.count <= 64, !name.contains("/"), !name.contains("\0"),
              name != ".", name != ".." else { throw AppUIDriverFailure.invalidFile }
    }
}
#endif
