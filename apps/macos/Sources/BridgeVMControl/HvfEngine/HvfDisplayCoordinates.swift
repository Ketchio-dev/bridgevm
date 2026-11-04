#if canImport(AppKit)
import CoreGraphics

/// Where the guest image sits in the display view: aspect fit and centred (the layer's
/// `.resizeAspect`), with its scale in view points per guest pixel.
struct HvfDisplayFit: Equatable {
    var rect: CGRect
    var scale: CGFloat

    init?(viewSize: CGSize, imageSize: CGSize) {
        guard viewSize.width.isFinite, viewSize.height.isFinite,
              imageSize.width.isFinite, imageSize.height.isFinite,
              viewSize.width > 0, viewSize.height > 0,
              imageSize.width > 0, imageSize.height > 0 else { return nil }
        let scale = min(viewSize.width / imageSize.width, viewSize.height / imageSize.height)
        let displayed = CGSize(width: imageSize.width * scale, height: imageSize.height * scale)
        guard displayed.width.isFinite, displayed.height.isFinite,
              displayed.width > 0, displayed.height > 0 else { return nil }
        let origin = CGPoint(
            x: (viewSize.width - displayed.width) / 2,
            y: (viewSize.height - displayed.height) / 2
        )
        self.rect = CGRect(origin: origin, size: displayed)
        self.scale = scale
    }

    /// On the image, edges included: where pointer input goes to the guest.
    func contains(_ location: CGPoint) -> Bool {
        location.x.isFinite && location.y.isFinite
            && location.x >= rect.minX && location.y >= rect.minY
            && location.x <= rect.maxX && location.y <= rect.maxY
    }
}

enum HvfDisplayCoordinates {
    static func absolutePointer(
        location: CGPoint,
        viewSize: CGSize,
        imageSize: CGSize
    ) -> (x: UInt16, y: UInt16)? {
        guard let fit = HvfDisplayFit(viewSize: viewSize, imageSize: imageSize),
              fit.contains(location) else { return nil }
        let x = ((location.x - fit.rect.minX) / fit.rect.width * 32_767).rounded()
        let y = ((location.y - fit.rect.minY) / fit.rect.height * 32_767).rounded()
        guard x.isFinite, y.isFinite else { return nil }
        return (UInt16(clamping: Int(x)), UInt16(clamping: Int(y)))
    }
}
#endif
