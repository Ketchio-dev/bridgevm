import CoreGraphics
import Foundation

/// Which `RESIZE WxH[@P]` the display window asks the guest display for (A8). The request is the view
/// size in points (1x), clamped to a sane guest mode range. With Retina resolution on (`retina`) and the
/// window on a HiDPI screen it is the view's backing pixels instead, with the screen's scale as P, up to
/// `retinaMaximum`; a larger window asks for its points at `@100`. A size goes out once the view has kept
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
    /// The largest Retina mode, in pixels: the signed display driver switched to 2560x1600 live but never
    /// listed 3456x2234 (#493's run), so a larger backing size asks for points at 100 %.
    static let retinaMaximum = Size(width: 2560, height: 1600)
    /// The scales Windows offers, in percent (`DESKTOP_SCALES` in the probe's desktop_scale.rs).
    static let scales = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500]
    static let settleDelay: TimeInterval = 0.4

    /// Retina resolution is on and the display is the 2D virtio-gpu.
    var retina = false
    private(set) var view: View?
    private(set) var sent: Size?
    /// A scale went out. The VMM sets its latest scale again after every switch, also for a plain
    /// `RESIZE WxH` and through an in-process guest reboot, so from then on a 1x request asks for 100 %.
    private(set) var scaleSent = false
    private var dueAt: TimeInterval?

    /// The request for the view as it is now; nil before it is laid out.
    var latest: Size? { view.flatMap { Self.size(for: $0, retina: retina, scaleSent: scaleSent) } }

    /// nil for an empty or non-finite view (not laid out yet).
    static func size(points: CGSize) -> Size? {
        guard points.width.isFinite, points.height.isFinite, points.width >= 1, points.height >= 1 else { return nil }
        func clamp(_ value: CGFloat, _ low: Int, _ high: Int) -> Int {
            Int(min(max(value.rounded(), CGFloat(low)), CGFloat(high)))
        }
        return Size(width: clamp(points.width, minimum.width, maximum.width),
                    height: clamp(points.height, minimum.height, maximum.height))
    }

    /// Off, or on a 1x screen, this is `size(points:)`, plain unless the device already got a scale.
    static func size(for view: View, retina: Bool, scaleSent: Bool = false) -> Size? {
        guard var size = size(points: view.points) else { return nil }
        guard retina else { return size }
        let percent = scale(view.scale)
        if percent == 100 {
            if scaleSent { size.scale = 100 }
            return size
        }
        let width = (view.points.width * view.scale).rounded(), height = (view.points.height * view.scale).rounded()
        guard width <= CGFloat(retinaMaximum.width), height <= CGFloat(retinaMaximum.height) else {
            size.scale = 100
            return size
        }
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
        if size.scale != nil { scaleSent = true }
    }
}
