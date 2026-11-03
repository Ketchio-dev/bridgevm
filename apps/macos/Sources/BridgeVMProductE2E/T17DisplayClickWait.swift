import CoreGraphics
import Foundation

/// Cooperative deadline checks cannot interrupt a synchronous accessibility read.
enum T17DisplayClickWait {
    static func perform(spot: CGPoint, timeout: TimeInterval, read: () -> T17DisplayClick.Target?,
                        post: (CGPoint) -> Bool, now: () -> Date, pause: () -> Void,
                        coveredBy: (CGPoint) -> String? = { _ in nil }) throws {
        let failure = T17Blocker(code: "ui-element-missing",
            detail: "guest display surface did not present a frame in the focused window")
        guard timeout.isFinite, timeout > 0 else { throw failure }
        let deadline = now().addingTimeInterval(timeout)
        var cover: String?
        repeat {
            guard now() < deadline else { throw cover.map(covered) ?? failure }
            let point = read().flatMap { T17DisplayClick.point(for: $0, at: spot) }
            guard now() < deadline else { throw cover.map(covered) ?? failure }
            // The click is a system-wide event: withhold it while another app's
            // window (for example the operator's own VM) is on top at that point.
            if let point, let owner = coveredBy(point) {
                cover = owner
            } else if let point {
                // The coverage query is synchronous too: an attempt it carried
                // past the deadline must not click.
                guard now() < deadline else { throw failure }
                guard post(point) else {
                    throw T17Blocker(code: "ui-element-missing", detail: "display click events could not be created")
                }
                return
            }
            pause()
        } while now() < deadline
        throw cover.map(covered) ?? failure
    }

    static func covered(_ owner: String) -> T17Blocker {
        T17Blocker(code: "display-covered", detail: "another app's window (\(owner)) covers the guest display; click withheld")
    }
}
