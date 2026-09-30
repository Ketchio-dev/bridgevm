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

    static func point(for target: Target) -> CGPoint? {
        guard target.value?.hasPrefix("frame ") == true, target.focused,
              let frame = target.frame, frame.width > 0, frame.height > 0 else { return nil }
        return CGPoint(x: frame.midX, y: frame.midY)
    }

    static func click(timeout: TimeInterval, read: () -> Target?, post: (CGPoint) -> Bool) throws {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if let point = read().flatMap(point(for:)) {
                guard post(point) else {
                    throw T17Blocker(code: "ui-element-missing", detail: "display click events could not be created")
                }
                return
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        throw T17Blocker(code: "ui-element-missing", detail: "guest display surface did not present a frame in the focused window")
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
