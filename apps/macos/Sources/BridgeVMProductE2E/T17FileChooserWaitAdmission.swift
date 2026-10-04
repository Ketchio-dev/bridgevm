import Foundation

enum T17FileChooserWaitAdmission {
    static func check(stage: String, deadline: TimeInterval, lastTransient: Error?,
                      failureContext: () -> String, now: () -> TimeInterval) throws {
        let time = now()
        guard time.isFinite, time < deadline else {
            let detail = (lastTransient as? T17Blocker).map { "; last_transient=\($0.detail)" } ?? ""
            throw T17FileChooser.failure("timed out waiting for \(stage)\(detail)" + failureContext())
        }
    }
}
