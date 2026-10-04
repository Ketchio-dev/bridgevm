import Foundation

enum T17ChooserStageRunner {
    static func run(stage: String, deadline: TimeInterval, timing: T17ChooserTiming,
                    failureContext: () -> String, now: () -> TimeInterval,
                    action: () throws -> Void) throws {
        try timing.run(stage) {
            try T17FileChooserDeadline.run(stage: stage, deadline: deadline,
                failureContext: failureContext, now: now, action: action)
        }
    }
}
