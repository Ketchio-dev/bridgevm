import Foundation

/// A completed read or action can consume the interaction's remaining time.
/// Check the shared deadline again before the next stage can send input.
enum T17FileChooserDeadline {
    static func run(stage: String, deadline: TimeInterval,
                    failureContext: () -> String, now: () -> TimeInterval,
                    action: () throws -> Void) throws {
        try T17FileChooserStage.run(stage) {
            guard now() < deadline else {
                throw T17FileChooser.failure("timed out before chooser stage" + failureContext())
            }
            try action()
        }
    }
}
