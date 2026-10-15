#if canImport(AppKit)
import Darwin

/// Owns the descriptor and mapping together; a replacement path is a different backing.
final class HvfFramebufferMapping {
    let pointer: UnsafeMutableRawPointer
    let length: Int
    private let descriptor: Int32

    init?(path: String) {
        let descriptor = HvfFramebufferFileIdentity.open(path)
        guard descriptor >= 0 else { return nil }
        guard let length = HvfFramebufferFileIdentity.length(descriptor, matching: path), length >= 64,
              let pointer = Darwin.mmap(nil, length, PROT_READ, MAP_SHARED, descriptor, 0),
              pointer != MAP_FAILED else {
            Darwin.close(descriptor)
            return nil
        }
        self.descriptor = descriptor
        self.pointer = pointer
        self.length = length
    }

    func currentLength(matching path: String?) -> Int? {
        HvfFramebufferFileIdentity.length(descriptor, matching: path)
    }

    deinit {
        Darwin.munmap(pointer, length)
        Darwin.close(descriptor)
    }
}
#endif
