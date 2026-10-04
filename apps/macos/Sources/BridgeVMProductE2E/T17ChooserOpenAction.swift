import ApplicationServices
import Foundation
enum T17ChooserOpenAction {
    static let targetRole = kAXButtonRole as String
    static func perform(identifier: String, deadline: TimeInterval,
                        enabled: () throws -> Bool?, press: () -> AXError,
                        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
                        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }) throws -> AXError {
        repeat {
            guard now() < deadline else {
                throw T17FileChooser.failure("chooser control admission timed out: \(identifier)")
            }
            guard let isEnabled = try enabled() else {
                throw T17FileChooser.failure("chooser control has no AXEnabled value: \(identifier)")
            }
            if now() >= deadline {
                let state = isEnabled ? "readiness timed out" : "remained disabled"
                throw T17FileChooser.failure("chooser control \(state): \(identifier)")
            }
            if !isEnabled { pause(); continue }
            let result = press()
            guard result == .success || result == .cannotComplete else {
                throw T17FileChooser.failure("chooser control AXPress failed: \(identifier); ax_error=\(result.rawValue)")
            }
            guard now() < deadline else { throw T17FileChooser.failure("chooser control AXPress returned after deadline: \(identifier); ax_error=\(result.rawValue)") }
            // cannotComplete remains provisional until exact panel/path/dismissal proof.
            return result
        } while true
    }
}
