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
            let prepared = try FirstRunImport.prepare(inputs, slug: VMLibrary.registrationSlug(for: inputs.displayName),
                libraryRoot: libraryRoot, snapshotHelper: snapshotHelper,
                converting: { progress(.converting($0, percent: $1)) })
            progress(.saving)
            return VMLibrary.windowsShareRegistrationLock.withLock {
                prepared.assignNewShare(in: libraryRoot)
                return finishPublication(publish(prepared.config, libraryRoot), prepared: prepared)
            }
        } catch is FirstRunImportDestination.NameInUse {
            return .failed("라이브러리에서 이미 사용 중인 VM 이름입니다. 다른 이름을 입력한 뒤 다시 가져오세요.")
        } catch {
            return .failed(WindowsFileFailureFeedback.reportImport("import", error))
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
            let message = (error as? VMRegistrationCollision)?.localizedDescription ?? WindowsFileFailureFeedback.publication
            return .failed(WindowsFileFailureFeedback.report("import-publication", error, sentence: message))
        case .publishedButUnsynced(let error):
            return .publishedButUnsynced(prepared.config,
                WindowsFileFailureFeedback.report("import-publication-sync", error, sentence: WindowsFileFailureFeedback.unsynced))
        }
    }
}
