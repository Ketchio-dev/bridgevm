import CoreGraphics
import Foundation

/// The display drops a click that arrives before its first guest frame, so the
/// click waits until the surface reports a presented frame in the focused window.
enum T17DisplayClick {
    static let surface = "bridgevm.runtime.display.surface"

    struct Target: Equatable {
        let value: String?
        let focused: Bool
        let frame: CGRect?
    }

    static func point(for target: Target, at spot: CGPoint = CGPoint(x: 0.5, y: 0.5)) -> CGPoint? {
        guard target.focused, let guest = T17DisplayImageRect.guestSize(target.value), let frame = target.frame else { return nil }
        return T17DisplayImageRect.point(spot, guest: guest, in: frame)
    }

    static func click(at spot: CGPoint, timeout: TimeInterval, read: () -> Target?, post: (CGPoint) -> Bool,
                      now: () -> Date = Date.init,
                      pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }) throws {
        try T17DisplayClickWait.perform(spot: spot, timeout: timeout, read: read, post: post, now: now, pause: pause)
    }

    static func post(_ point: CGPoint) -> Bool {
        let events = [CGEventType.mouseMoved, .leftMouseDown, .leftMouseUp].compactMap {
            CGEvent(mouseEventSource: nil, mouseType: $0, mouseCursorPosition: point, mouseButton: .left)
        }
        guard events.count == 3 else { return false }
        events.forEach { $0.post(tap: .cghidEventTap) }
        return true
    }
}
