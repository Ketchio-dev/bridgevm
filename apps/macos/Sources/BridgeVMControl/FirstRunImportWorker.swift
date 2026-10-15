import Foundation

enum FirstRunImportWorker {
    /// Runs entirely on the worker. Published or conflicting registrations
    /// preserve prepared files; undisputed prepublication failures roll back.
    static func run(
        _ inputs: FirstRunImport.Inputs, libraryRoot: URL,
        snapshotHelper: URL = HvfMediaImportHelper.bundled,
        progress: @Sendable (FirstRunImportStage) -> Void = { _ in },
        publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { VMLibrary.createOutcome($0, rootURL: $1) }
    ) -> FirstRunImportResult {
        progress(.validating)
        if let error = FirstRunImport.validate(inputs) { return .failed(error.description) }
        do {
            progress(.copying)
            let prepared = try FirstRunImport.prepare(inputs, slug: VMConfig.slugify(inputs.displayName),
                libraryRoot: libraryRoot, snapshotHelper: snapshotHelper)
            progress(.saving)
            return finishPublication(publish(prepared.config, libraryRoot), prepared: prepared)
        } catch {
            return .failed("VM 번들을 만들지 못했습니다: \(error.localizedDescription)")
        }
    }
}
