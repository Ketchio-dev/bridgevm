import ApplicationServices
import Foundation

enum T17ChooserOpenAction {
    static let targetRole = kAXButtonRole as String

    static func perform(identifier: String, timeout: TimeInterval,
                        enabled: () throws -> Bool?, press: () -> AXError,
                        now: () -> Date = Date.init,
                        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }) throws -> AXError {
        let deadline = now().addingTimeInterval(timeout)
        repeat {
            guard let isEnabled = try enabled() else {
                throw T17FileChooser.failure("chooser control has no AXEnabled value: \(identifier)")
            }
            if !isEnabled {
                if now() >= deadline {
                    throw T17FileChooser.failure("chooser control remained disabled: \(identifier); timeout_s=\(timeout)")
                }
                pause(); continue
            }
            let result = press()
            guard result == .success || result == .cannotComplete else {
                throw T17FileChooser.failure("chooser control AXPress failed: \(identifier); ax_error=\(result.rawValue)")
            }
            // cannotComplete is only a provisional handoff. The chooser state
            // machine must still prove the exact panel, path and dismissal.
            return result
        } while true
    }
}
