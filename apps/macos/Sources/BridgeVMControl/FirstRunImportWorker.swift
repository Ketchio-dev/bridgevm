import Foundation

enum FirstRunImportWorker {
    /// Runs entirely on the worker. Published or conflicting registrations
    /// preserve prepared files; undisputed prepublication failures roll back.
    static func run(
        _ inputs: FirstRunImport.Inputs, libraryRoot: URL,
        snapshotHelper: URL = HvfMediaImportHelper.bundled,
        progress: @escaping @Sendable (FirstRunImportStage) -> Void = { _ in },
        publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { VMLibrary.createOutcome($0, rootURL: $1) }
    ) -> FirstRunImportResult {
        progress(.validating)
        if let error = FirstRunImport.validate(inputs) { return .failed(error.description) }
        do {
            progress(HvfImportedDiskFormat.detect(path: inputs.diskPath).map { .converting($0, percent: 0) } ?? .copying)
            let prepared = try FirstRunImport.prepare(inputs, slug: VMConfig.slugify(inputs.displayName),
                libraryRoot: libraryRoot, snapshotHelper: snapshotHelper,
                converting: { progress(.converting($0, percent: $1)) })
            progress(.saving)
            return VMLibrary.windowsShareRegistrationLock.withLock {
                prepared.assignNewShare(in: libraryRoot)
                return finishPublication(publish(prepared.config, libraryRoot), prepared: prepared)
            }
        } catch {
            return .failed(WindowsFileFailureFeedback.report("import", error, sentence: WindowsFileFailureFeedback.importing))
        }
    }
}

extension FirstRunImportWorker {
    static func finishPublication(_ outcome: VMRegistrationCommitOutcome,
        prepared: FirstRunImportPreparation) -> FirstRunImportResult {
        if outcome.preservesPreparedDestination { prepared.preserve() }
        switch outcome {
        case .committed:
            return .committed(prepared.config)
        case .notPublished(let error):
            return .failed(WindowsFileFailureFeedback.report("import-publication", error, sentence: WindowsFileFailureFeedback.publication))
        case .publishedButUnsynced(let error):
            return .publishedButUnsynced(prepared.config,
                WindowsFileFailureFeedback.report("import-publication-sync", error, sentence: WindowsFileFailureFeedback.unsynced))
        }
    }
}
