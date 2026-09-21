import Foundation

enum T17FileChooserWait {
    static func until(
        stage: String, deadline: TimeInterval,
        failureContext: () -> String,
        now: () -> TimeInterval,
        pause: () -> Void,
        ready: () throws -> Bool
    ) throws {
        var lastTransient: Error?
        repeat {
            var isReady = false
            do {
                isReady = try ready()
            } catch where T17FileChooserSnapshot.isTransientReadFailure(error) {
                lastTransient = error
            }
            if now() >= deadline {
                let detail = (lastTransient as? T17Blocker).map {
                    "; last_transient=\($0.detail)"
                } ?? ""
                throw T17FileChooser.failure(
                    "timed out waiting for \(stage)\(detail)" + failureContext())
            }
            if isReady { return }
            pause()
        } while true
    }
}
