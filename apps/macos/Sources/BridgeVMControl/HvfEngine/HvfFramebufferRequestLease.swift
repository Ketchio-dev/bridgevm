#if canImport(AppKit)
import Darwin

/// A request descriptor belongs to the mapping it was opened for, never just its pathname.
final class HvfFramebufferRequestLease {
    private var descriptor: Int32 = -1
    private weak var mapping: HvfFramebufferMapping?

    func descriptor(for mapping: HvfFramebufferMapping, path: String) -> Int32? {
        if self.mapping !== mapping { reset() }
        if descriptor >= 0 { return descriptor }
        let opened = Darwin.open(path, O_WRONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC)
        guard opened >= 0 else { return nil }
        guard HvfFramebufferFileIdentity.length(opened, matching: path) != nil, mapping.owns(opened) else {
            Darwin.close(opened)
            return nil
        }
        descriptor = opened
        self.mapping = mapping
        return opened
    }

    func reset() {
        if descriptor >= 0 { Darwin.close(descriptor) }
        descriptor = -1
        mapping = nil
    }

    deinit { reset() }
}
#endif
