import ApplicationServices
import Foundation

enum T17PressAction {
    static let targetRole = kAXButtonRole as String

    static func perform(identifier: String, timeout: TimeInterval, enabled: () throws -> Bool?,
                        press: () -> AXError, activate: () -> Bool, retry: () -> AXError,
                        frontmost: () -> Bool, now: () -> Date = Date.init,
                        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }) throws {
        let deadline = now().addingTimeInterval(timeout)
        repeat {
            guard let isEnabled = try enabled() else {
                throw T17Blocker(code: "ui-element-missing", detail: "identified UI element has no AXEnabled value: \(identifier)")
            }
            guard isEnabled else { pause(); continue }
            let first = press()
            if first == .success { return }
            guard first == .cannotComplete else { throw failure(identifier, first, nil, nil, 1, frontmost()) }
            let activated = activate()
            guard activated else { throw failure(identifier, first, nil, false, 1, frontmost()) }
            var last: AXError?, attempts = 1
            repeat {
                pause(); last = retry(); attempts += 1
                if last == .success { return }
            } while last == .cannotComplete && now() < deadline
            throw failure(identifier, first, last, true, attempts, frontmost())
        } while now() < deadline
        throw T17Blocker(code: "ui-element-missing", detail: "identified UI element remained disabled: \(identifier); timeout_s=\(timeout)")
    }

    private static func failure(_ id: String, _ first: AXError, _ last: AXError?, _ activated: Bool?, _ attempts: Int, _ frontmost: Bool) -> T17Blocker {
        T17Blocker(code: "ui-element-missing", detail: "AXPress failed: \(id); first_ax_error=\(first.rawValue); retry_ax_error=\(last.map { String($0.rawValue) } ?? "not-attempted"); activation_succeeded=\(activated.map { String($0) } ?? "not-attempted"); frontmost=\(frontmost); attempts=\(attempts)")
    }
}
