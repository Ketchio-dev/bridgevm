import Foundation

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
