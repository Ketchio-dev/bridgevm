import CoreGraphics
import Foundation

/// The display letterboxes the guest image, so a spot on the guest screen maps
/// through the aspect-fitted image rectangle, not the whole surface.
enum T17DisplayImageRect {
    /// The guest size from the surface's accessibility value, `frame <width>x<height>`.
    static func guestSize(_ value: String?) -> CGSize? {
        guard let value, value.hasPrefix("frame ") else { return nil }
        let parts = value.dropFirst(6).split(separator: "x", omittingEmptySubsequences: false)
        guard parts.count == 2, let width = Int(parts[0]), let height = Int(parts[1]), width > 0, height > 0 else { return nil }
        return CGSize(width: width, height: height)
    }

    static func point(_ spot: CGPoint, guest: CGSize, in surface: CGRect) -> CGPoint? {
        guard surface.width > 0, surface.height > 0, (0...1).contains(spot.x), (0...1).contains(spot.y) else { return nil }
        let scale = min(surface.width / guest.width, surface.height / guest.height)
        let size = CGSize(width: guest.width * scale, height: guest.height * scale)
        let origin = CGPoint(x: surface.midX - size.width / 2, y: surface.midY - size.height / 2)
        return CGPoint(x: origin.x + spot.x * size.width, y: origin.y + spot.y * size.height)
    }
}
