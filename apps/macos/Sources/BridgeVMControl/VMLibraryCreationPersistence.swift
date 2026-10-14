import Foundation

extension VMLibrary {
    /// New VM publication must not replace a registration created while its
    /// media was being prepared, even when the storage roots are different.
    static func createOutcome(
        _ config: VMConfig, rootURL: URL = root,
        publish: (Data, URL) -> VMRegistrationCommitOutcome = {
            VMRegistrationWriter.commit($0, to: $1, replaceExisting: false)
        }
    ) -> VMRegistrationCommitOutcome {
        saveOutcome(config, rootURL: rootURL, publish: publish)
    }

    static func createRegistration(_ config: VMConfig, rootURL: URL = root,
        preserving preserve: () -> Void = {}) -> Bool {
        let outcome = createOutcome(config, rootURL: rootURL)
        if outcome.preservesPreparedDestination { preserve() }
        return outcome.isCommitted
    }
}
