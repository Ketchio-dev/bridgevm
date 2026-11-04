import Darwin
import Foundation

/// Copies dropped Finder items into a private staging folder the runtime streams to the guest
/// (file_drop.rs). The app holds the drag's access to the files, so the runtime never reads the
/// user's folders; the copy is also a snapshot that cannot change mid-transfer.
enum HvfFileDropStaging {
    struct Summary: Equatable {
        var files = 0
        var folders = 0
        var bytes: Int64 = 0
        /// APFS clones (instant, no extra space) and plain copies (another volume).
        var cloned = 0
        var copied = 0
        /// Symlinks, devices and other non-regular items, never followed.
        var skipped = 0
    }

    /// Stages `items` into `root` (created here) under Windows-safe, case-insensitively unique
    /// names. Only regular files and folders are copied; a symlink anywhere, even a dropped item
    /// itself, is skipped and never followed.
    static func stage(_ items: [URL], into root: URL, clone: @escaping (String, String) -> Bool = cloneFile) throws -> Summary {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        var walk = Walk(clone: clone)
        // A dropped folder may contain the VM bundle (the home folder, say), and with it this
        // staging folder: skip it and its drops/ parent, by identity, or the copy feeds itself.
        for path in [root.path, root.deletingLastPathComponent().path] {
            var info = stat()
            if lstat(path, &info) == 0 { walk.excluded.insert(Identity(info)) }
        }
        var taken = Set<String>()
        for item in items {
            try copy(item.path, into: root.path, taken: &taken, walk: &walk)
        }
        return walk.summary
    }

    private struct Identity: Hashable {
        let device: Int64
        let inode: UInt64
        init(_ info: stat) {
            device = Int64(info.st_dev)
            inode = UInt64(info.st_ino)
        }
    }

    private struct Walk {
        let clone: (String, String) -> Bool
        var excluded = Set<Identity>()
        var summary = Summary()
    }

    /// An APFS clone, or false (another volume, or no clone support) without leaving a file.
    static func cloneFile(_ source: String, _ target: String) -> Bool {
        copyfile(source, target, nil, copyfile_flags_t(COPYFILE_CLONE_FORCE)) == 0
    }

    private static func copy(_ source: String, into parent: String, taken: inout Set<String>,
                             walk: inout Walk) throws {
        let leaf = (source as NSString).lastPathComponent
        var info = stat()
        guard leaf != ".DS_Store", lstat(source, &info) == 0, !walk.excluded.contains(Identity(info)) else { return }
        let kind = info.st_mode & S_IFMT
        guard kind == S_IFDIR || kind == S_IFREG else {
            walk.summary.skipped += 1
            return
        }
        let name = HvfFileDropNames.unique(HvfFileDropNames.windowsName(leaf),
                                           isDirectory: kind == S_IFDIR, taken: &taken)
        let target = (parent as NSString).appendingPathComponent(name)
        if kind == S_IFDIR {
            guard mkdir(target, 0o700) == 0 else { throw POSIXError(.init(rawValue: errno) ?? .EIO) }
            walk.summary.folders += 1
            var inner = Set<String>()
            for child in try FileManager.default.contentsOfDirectory(atPath: source).sorted() {
                try copy((source as NSString).appendingPathComponent(child), into: target,
                         taken: &inner, walk: &walk)
            }
            return
        }
        if walk.clone(source, target) {
            walk.summary.cloned += 1
        } else {
            // Another volume (or a file system without clones): a plain data copy.
            unlink(target)
            let flags = copyfile_flags_t(COPYFILE_DATA | COPYFILE_EXCL | COPYFILE_NOFOLLOW_SRC)
            guard copyfile(source, target, nil, flags) == 0 else {
                throw POSIXError(.init(rawValue: errno) ?? .EIO)
            }
            walk.summary.copied += 1
        }
        walk.summary.files += 1
        walk.summary.bytes += Int64(info.st_size)
    }
}
