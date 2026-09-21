import ApplicationServices
import Foundation

enum T17PressAction {
    static func perform(identifier: String, timeout: TimeInterval,
                        enabled: () throws -> Bool?,
                        press: () -> AXError,
                        activate: () -> Bool,
                        retry: () -> AXError,
                        frontmost: () -> Bool,
                        now: () -> Date = Date.init,
                        pause: () -> Void = {
                            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
                        }) throws {
        let deadline = now().addingTimeInterval(timeout)
        repeat {
            guard let isEnabled = try enabled() else {
                throw T17Blocker(code: "ui-element-missing",
                    detail: "identified UI element has no AXEnabled value: \(identifier)")
            }
            if isEnabled {
                let first = press()
                if first == .success { return }
                let activated = activate()
                let second = activated ? retry() : nil
                guard second == .success else {
                    throw T17Blocker(code: "ui-element-missing",
                        detail: "AXPress failed: \(identifier); first_ax_error=\(first.rawValue); retry_ax_error=\(second.map { String($0.rawValue) } ?? "not-attempted"); activation_succeeded=\(activated); frontmost=\(frontmost())")
                }
                return
            }
            pause()
        } while now() < deadline
        throw T17Blocker(code: "ui-element-missing",
            detail: "identified UI element remained disabled: \(identifier); timeout_s=\(timeout)")
    }
}
