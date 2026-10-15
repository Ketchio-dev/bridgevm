#if canImport(AppKit)
import AppKit
import QuartzCore

extension FBLayerView {
    func startDisplayLink() {
        guard frameDisplayLink == nil else { return }
        if #available(macOS 14.0, *) {
            let link = displayLink(target: self, selector: #selector(step(_:)))
            frameDisplayLink = link
            link.add(to: .main, forMode: .common)
        }
    }

    func stopDisplayLink() {
        if #available(macOS 14.0, *) {
            frameDisplayLink?.invalidate()
            frameDisplayLink = nil
        }
    }
}
#endif
