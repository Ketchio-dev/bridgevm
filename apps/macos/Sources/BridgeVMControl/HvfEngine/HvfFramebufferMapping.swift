#if canImport(AppKit)
import Darwin

/// Owns the descriptor and mapping together; a replacement path is a different backing.
final class HvfFramebufferMapping {
    let pointer: UnsafeMutableRawPointer
    let length: Int
    private let descriptor: Int32

    init?(path: String) {
        guard let mapped = HvfFramebufferMappedRegion.open(path) else { return nil }
        descriptor = mapped.descriptor
        pointer = mapped.pointer
        length = mapped.length
    }

    func owns(_ candidate: Int32) -> Bool {
        HvfFramebufferMappedRegion.sameBacking(descriptor, candidate)
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
