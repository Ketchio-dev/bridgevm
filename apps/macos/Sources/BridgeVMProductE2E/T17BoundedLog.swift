import Foundation

/// Bounded, symlink-refusing reads of lane files for T17 waits.
enum T17BoundedLog {
    static func regularFile(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]) else { return false }
        return values.isRegularFile == true && values.isSymbolicLink != true && (values.fileSize ?? 0) > 0
    }

    static func lines(_ url: URL) -> [String] {
        text(url).split(whereSeparator: \.isNewline).map(String.init)
    }

    /// The last 8 MiB, or with `whole` the entire file when it is at most 64 MiB.
    static func text(_ url: URL, whole: Bool = false) -> String {
        guard regularFile(url), let handle = FileHandle(forReadingAtPath: url.path) else { return "" }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        guard !whole || size <= 64 * 1024 * 1024 else { return "" }
        try? handle.seek(toOffset: !whole && size > 8 * 1024 * 1024 ? size - 8 * 1024 * 1024 : 0)
        return String(decoding: (try? handle.readToEnd()) ?? Data(), as: UTF8.self)
    }
}
