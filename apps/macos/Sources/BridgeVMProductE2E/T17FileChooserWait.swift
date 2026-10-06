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
        func check() throws {
            try T17FileChooserWaitAdmission.check(stage: stage, deadline: deadline,
                lastTransient: lastTransient, failureContext: failureContext, now: now)
        }
        repeat {
            try check()
            var isReady = false
            do {
                isReady = try ready()
            } catch where T17FileChooserSnapshot.isTransientReadFailure(error) {
                lastTransient = error
            }
            try check()
            if isReady { return }
            pause()
            try check()
        } while true
    }
}
