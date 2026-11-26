import Foundation

enum HVFCreationOutcome: Sendable {
    case created(VMConfig)
    case publishedButUnsynced(VMConfig, String)
    /// Nothing was published; the sentence tells the user why and what to do.
    case refused(String)
}

extension HVFCreationOutcome {
    /// A collision remains a failed creation, while its prepared files survive
    /// independently of subsequent changes to the competing registration.
    static func publish(
        _ config: VMConfig, libraryRoot: URL, persist: Bool,
        preserving preserve: () -> Void = {},
        using publisher: (VMConfig, URL) -> VMRegistrationCommitOutcome
    ) -> HVFCreationOutcome? {
        guard persist else { return .created(config) }
        let outcome = publisher(config, libraryRoot)
        if outcome.preservesPreparedDestination { preserve() }
        switch outcome {
        case .committed:
            return .created(config)
        case .notPublished:
            return nil
        case .publishedButUnsynced:
            return .publishedButUnsynced(config, WindowsFileFailureFeedback.unsynced)
        }
    }
}
