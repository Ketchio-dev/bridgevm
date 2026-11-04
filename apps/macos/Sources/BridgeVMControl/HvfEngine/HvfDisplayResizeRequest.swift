import CoreGraphics
import Foundation

/// Which `RESIZE WxH[@P]` the display window asks the guest display for (A8). The request is the view
/// size in points (1x), clamped to a sane guest mode range. A VM whose Retina setting was ever set
/// (`retina` not nil) names the Windows scale in every request, because Windows keeps a scale across
/// mode switches (#493's live runs) and, by its persistence, restarts. With Retina resolution on and the
/// window on a HiDPI screen the request is the view's backing pixels at the screen's scale, up to
/// `retinaMaximumPixels`; otherwise it is the view's points at `@100`. A size goes out once the view has kept
/// it for `settleDelay`, or at once when a live resize ends, and never twice in a row unless the display
/// device restarted at its default size (new run or guest reboot).
struct HvfDisplayResizeRequest {
    struct Size: Equatable {
        let width: Int
        let height: Int
        /// The Windows display scale in percent; nil leaves Windows' scale as it is.
        var scale: Int? = nil
        var command: String { "RESIZE \(width)x\(height)" + (scale.map { "@\($0)" } ?? "") }
    }

    /// The view's size in points and its window's backing scale factor.
    struct View: Equatable {
        let points: CGSize
        let scale: CGFloat
    }

    static let minimum = Size(width: 640, height: 480)
    static let maximum = Size(width: 3840, height: 2160)
    /// The most pixels a Retina mode may have. #493's second live run switched 3456x2234@200 and
    /// 3024x1964@200 live once the EDID listed 3840x2160, for which viogpudo reserves 33,177,600 bytes
    /// (3840x2160 at 4 bytes each). A larger backing size, or a side above `maximum.width`, asks for
    /// points at 100 %.
    static let retinaMaximumPixels = 3840 * 2160
    /// The scales Windows offers, in percent (`DESKTOP_SCALES` in the probe's desktop_scale.rs).
    static let scales = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500]
    static let settleDelay: TimeInterval = 0.4

    /// The VM's Retina setting for this display: nil never set (plain requests); true on, with the 2D
    /// virtio display; false otherwise, as after turning it off (points at 100 %).
    var retina: Bool?
    private(set) var view: View?
    private(set) var sent: Size?
    private var dueAt: TimeInterval?

    /// The request for the view as it is now; nil before it is laid out.
    var latest: Size? { view.flatMap { Self.size(for: $0, retina: retina) } }

    /// nil for an empty or non-finite view (not laid out yet).
    static func size(points: CGSize) -> Size? {
        guard points.width.isFinite, points.height.isFinite, points.width >= 1, points.height >= 1 else { return nil }
        func clamp(_ value: CGFloat, _ low: Int, _ high: Int) -> Int {
            Int(min(max(value.rounded(), CGFloat(low)), CGFloat(high)))
        }
        return Size(width: clamp(points.width, minimum.width, maximum.width),
                    height: clamp(points.height, minimum.height, maximum.height))
    }

    /// Never set: `size(points:)`, plain. Set: the points at 100 %, or for `true` on a HiDPI screen within
    /// the cap the backing pixels at the screen's scale.
    static func size(for view: View, retina: Bool?) -> Size? {
        guard var size = size(points: view.points) else { return nil }
        guard let retina else { return size }
        size.scale = 100
        let percent = scale(view.scale)
        guard retina, percent != 100 else { return size }
        let width = (view.points.width * view.scale).rounded(), height = (view.points.height * view.scale).rounded()
        let side = CGFloat(maximum.width)
        guard width <= side, height <= side, width * height <= CGFloat(retinaMaximumPixels) else { return size }
        return Size(width: max(Int(width), minimum.width), height: max(Int(height), minimum.height), scale: percent)
    }

    /// The ladder step nearest round(scale × 100); 100 for a non-finite scale.
    static func scale(_ factor: CGFloat) -> Int {
        guard factor.isFinite else { return 100 }
        let percent = Int((factor * 100).rounded())
        return scales.min { abs($0 - percent) < abs($1 - percent) } ?? 100
    }

    /// Records a laid-out view and restarts the trailing debounce when its request changed.
    mutating func viewResized(to view: View, at now: TimeInterval) {
        guard Self.size(points: view.points) != nil else { return }
        let before = latest
        self.view = view
        if latest != before { dueAt = now + Self.settleDelay }
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
