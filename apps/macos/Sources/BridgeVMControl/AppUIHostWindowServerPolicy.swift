#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import Darwin

enum AppUIHostWindowServerPolicy {
    struct Candidate {
        let windowID: UInt32
        let processID: Int?
        let onScreen: Bool
    }
    struct Dimensions: Equatable {
        let width: Int
        let height: Int
    }
    static func select(_ candidates: [Candidate], windowNumber: Int, processID: Int) throws -> Int {
        guard windowNumber > 0, windowNumber <= Int(UInt32.max), processID > 1, candidates.count <= 256 else {
            throw AppUIHostError.refused("WindowServer selection inputs exceed the owned window bounds")
        }
        let matches = candidates.indices.filter { candidates[$0].windowID == UInt32(windowNumber) }
        guard matches.count == 1, let index = matches.first,
              candidates[index].processID == processID, candidates[index].onScreen else {
            throw AppUIHostError.refused("WindowServer did not expose exactly one identified owned visible window")
        }
        return index
    }
    static func dimensions(x: Double, y: Double, width: Double, height: Double, scale: Double) throws -> Dimensions {
        guard [x, y, width, height, scale].allSatisfy(\.isFinite), width > 0, height > 0, scale > 0 else {
            throw AppUIHostError.refused("WindowServer content dimensions are invalid")
        }
        let pixelsWide = (width * scale).rounded(.up), pixelsHigh = (height * scale).rounded(.up)
        guard pixelsWide.isFinite, pixelsHigh.isFinite, pixelsWide >= 1, pixelsHigh >= 1,
              pixelsWide <= 8192, pixelsHigh <= 8192, pixelsWide * pixelsHigh <= 8_388_608 else {
            throw AppUIHostError.refused("WindowServer image exceeds the bounded pixel allocation")
        }
        return Dimensions(width: Int(pixelsWide), height: Int(pixelsHigh))
    }

    struct Output {
        let directory: URL
        private let parent: URL
        private let parentIdentity: stat
        private let directoryIdentity: stat

        init(parent: URL) throws {
            self.parent = parent
            parentIdentity = try Self.directoryInfo(parent)
            directory = parent.appendingPathComponent("window-server", isDirectory: true)
            guard mkdir(directory.path, 0o700) == 0 else {
                throw AppUIHostError.refused("WindowServer output must be a new private directory")
            }
            directoryIdentity = try Self.directoryInfo(directory)
        }
        func validate() throws {
            let currentParent = try Self.directoryInfo(parent), current = try Self.directoryInfo(directory)
            guard currentParent.st_dev == parentIdentity.st_dev, currentParent.st_ino == parentIdentity.st_ino,
                  current.st_dev == directoryIdentity.st_dev, current.st_ino == directoryIdentity.st_ino else {
                throw AppUIHostError.refused("WindowServer output directory ownership changed")
            }
        }
        func write(_ data: Data, name: String) throws {
            let limit = name == "owned-dark-default.png" ? 32 * 1024 * 1024 : 64 * 1024
            guard ["owned-dark-default.png", "owned-dark-default.json"].contains(name),
                  !data.isEmpty, data.count <= limit else {
                throw AppUIHostError.refused("WindowServer output name or byte bound differs")
            }
            try validate()
            let directoryFD = open(directory.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            guard directoryFD >= 0 else { throw AppUIHostError.refused("WindowServer output cannot be opened") }
            defer { close(directoryFD) }
            var opened = stat()
            guard fstat(directoryFD, &opened) == 0, opened.st_dev == directoryIdentity.st_dev,
                  opened.st_ino == directoryIdentity.st_ino else {
                throw AppUIHostError.refused("WindowServer output descriptor identity changed")
            }
            let file = openat(directoryFD, name, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
            guard file >= 0 else { throw AppUIHostError.refused("WindowServer refuses an existing output file") }
            defer { close(file) }
            try data.withUnsafeBytes { bytes in
                var offset = 0
                while offset < bytes.count {
                    let count = Darwin.write(file, bytes.baseAddress!.advanced(by: offset), bytes.count - offset)
                    if count < 0 && errno == EINTR { continue }
                    guard count > 0 else { throw AppUIHostError.refused("WindowServer output write failed") }
                    offset += count
                }
            }
            try validate()
        }
        private static func directoryInfo(_ url: URL) throws -> stat {
            var info = stat()
            guard url.isFileURL, url.path.hasPrefix("/"), url.standardizedFileURL.path == url.path,
                  url.resolvingSymlinksInPath().path == url.path, lstat(url.path, &info) == 0,
                  info.st_mode & S_IFMT == S_IFDIR, info.st_mode & 0o777 == 0o700, info.st_uid == geteuid() else {
                throw AppUIHostError.refused("WindowServer output requires an owned canonical 0700 directory")
            }
            return info
        }
    }
}
#endif
