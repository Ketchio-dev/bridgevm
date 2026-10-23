#if canImport(AppKit)
/// The display view's mapping of the BVFB file, split out of HvfFramebufferView.swift.
extension FBLayerView {
    func ensureMapping() -> Bool {
        if let mapping = framebufferMapping {
            guard let currentLength = mapping.currentLength(matching: framebufferPath) else {
                resetMapping()
                return false
            }
            if currentLength == mapping.length { return true }
            resetMapping()
        }
        guard let framebufferPath else { return false }
        framebufferMapping = HvfFramebufferMapping(path: framebufferPath)
        return framebufferMapping != nil
    }

    func resetMapping() {
        framebufferMapping = nil
        frameRequester.reset()
        lastProcessedSeq = .max
    }
}
#endif
