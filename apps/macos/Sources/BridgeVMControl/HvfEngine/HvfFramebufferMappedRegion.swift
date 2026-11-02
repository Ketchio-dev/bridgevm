#if canImport(AppKit)
import Darwin

/// Opens the mapping as one owned region and compares descriptors without a path race.
enum HvfFramebufferMappedRegion {
    static func open(_ path: String) -> (descriptor: Int32, pointer: UnsafeMutableRawPointer, length: Int)? {
        let descriptor = HvfFramebufferFileIdentity.open(path)
        guard descriptor >= 0 else { return nil }
        guard let length = HvfFramebufferFileIdentity.length(descriptor, matching: path), length >= 64,
              let pointer = Darwin.mmap(nil, length, PROT_READ, MAP_SHARED, descriptor, 0),
              pointer != MAP_FAILED else {
            Darwin.close(descriptor)
            return nil
        }
        return (descriptor, pointer, length)
    }

    static func sameBacking(_ first: Int32, _ second: Int32) -> Bool {
        var lhs = stat(), rhs = stat()
        return Darwin.fstat(first, &lhs) == 0 && Darwin.fstat(second, &rhs) == 0
            && lhs.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG)
            && rhs.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG)
            && lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
    }
}
#endif
