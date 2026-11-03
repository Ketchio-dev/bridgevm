import CoreGraphics
import Foundation

/// Which `RESIZE WxH` the display window asks the guest display for (A8). The request is the view
/// size in points (1x), clamped to a sane guest mode range. A size goes out once the view has kept it
/// for `settleDelay`, or at once when a live resize ends, and never twice in a row unless the display
/// device restarted at its default size (new run or guest reboot).
struct HvfDisplayResizeRequest {
    struct Size: Equatable {
        let width: Int
        let height: Int
        var command: String { "RESIZE \(width)x\(height)" }
    }

    static let minimum = Size(width: 640, height: 480)
    static let maximum = Size(width: 3840, height: 2160)
    static let settleDelay: TimeInterval = 0.4

    private(set) var latest: Size?
    private(set) var sent: Size?
    private var dueAt: TimeInterval?

    /// nil for an empty or non-finite view (not laid out yet).
    static func size(points: CGSize) -> Size? {
        guard points.width.isFinite, points.height.isFinite, points.width >= 1, points.height >= 1 else { return nil }
        func clamp(_ value: CGFloat, _ low: Int, _ high: Int) -> Int {
            Int(min(max(value.rounded(), CGFloat(low)), CGFloat(high)))
        }
        return Size(width: clamp(points.width, minimum.width, maximum.width),
                    height: clamp(points.height, minimum.height, maximum.height))
    }

    /// Records the view size and restarts the trailing debounce when the clamped size changed.
    mutating func viewResized(to points: CGSize, at now: TimeInterval) {
        guard let size = Self.size(points: points), size != latest else { return }
        latest = size
        dueAt = now + Self.settleDelay
    }

    /// A live resize ended: a pending size is due now instead of after the debounce.
    mutating func settle() {
        if dueAt != nil { dueAt = -.infinity }
    }

    /// The display device starts at its default size, so the latest size is due again now.
    mutating func deviceRestarted() {
        sent = nil
        dueAt = -.infinity
    }

    /// The size to write now, if any; the caller reports a successful write with `didSend`.
    mutating func take(at now: TimeInterval) -> Size? {
        guard let dueAt, now >= dueAt else { return nil }
        self.dueAt = nil
        return latest == sent ? nil : latest
    }

    mutating func didSend(_ size: Size) {
        sent = size
    }
}
