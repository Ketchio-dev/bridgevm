import CoreGraphics
import Foundation

/// Cooperative deadline checks cannot interrupt a synchronous accessibility read.
enum T17DisplayClickWait {
    static func perform(spot: CGPoint, timeout: TimeInterval, read: () -> T17DisplayClick.Target?,
                        post: (CGPoint) -> Bool, now: () -> Date, pause: () -> Void) throws {
        let failure = T17Blocker(code: "ui-element-missing",
            detail: "guest display surface did not present a frame in the focused window")
        guard timeout.isFinite, timeout > 0 else { throw failure }
        let deadline = now().addingTimeInterval(timeout)
        repeat {
            guard now() < deadline else { throw failure }
            let point = read().flatMap { T17DisplayClick.point(for: $0, at: spot) }
            guard now() < deadline else { throw failure }
            if let point {
                guard post(point) else {
                    throw T17Blocker(code: "ui-element-missing", detail: "display click events could not be created")
                }
                return
            }
            pause()
        } while now() < deadline
        throw failure
    }
}
