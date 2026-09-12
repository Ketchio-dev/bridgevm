#if canImport(AppKit)
import CoreGraphics

enum HvfDisplayCoordinates {
    static func absolutePointer(
        location: CGPoint,
        viewSize: CGSize,
        imageSize: CGSize
    ) -> (x: UInt16, y: UInt16)? {
        guard location.x.isFinite, location.y.isFinite,
              viewSize.width.isFinite, viewSize.height.isFinite,
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
        guard location.x >= origin.x, location.y >= origin.y,
              location.x <= origin.x + displayed.width,
              location.y <= origin.y + displayed.height else { return nil }
        let x = ((location.x - origin.x) / displayed.width * 32_767).rounded()
        let y = ((location.y - origin.y) / displayed.height * 32_767).rounded()
        guard x.isFinite, y.isFinite else { return nil }
        return (UInt16(clamping: Int(x)), UInt16(clamping: Int(y)))
    }
}
#endif
