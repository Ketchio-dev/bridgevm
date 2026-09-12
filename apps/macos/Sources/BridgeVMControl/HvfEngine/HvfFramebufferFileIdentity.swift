#if canImport(AppKit)
import Darwin

enum HvfFramebufferFileIdentity {
    static func open(_ path: String) -> Int32 {
        Darwin.open(path, O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC)
    }

    static func length(_ descriptor: Int32, matching path: String?) -> Int? {
        guard let path else { return nil }
        var opened = stat(), named = stat()
        guard Darwin.fstat(descriptor, &opened) == 0, Darwin.lstat(path, &named) == 0,
              opened.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              named.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              opened.st_dev == named.st_dev, opened.st_ino == named.st_ino,
              opened.st_size >= 0 else { return nil }
        let length = UInt64(opened.st_size)
        guard length <= UInt64(Int.max) else { return nil }
        return Int(length)
    }
}
#endif
