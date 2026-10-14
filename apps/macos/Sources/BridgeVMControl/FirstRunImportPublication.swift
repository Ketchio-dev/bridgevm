import Foundation

extension FirstRunImportWorker {
    static func finishPublication(_ outcome: VMRegistrationCommitOutcome,
        prepared: FirstRunImportPreparation) -> FirstRunImportResult {
        if outcome.preservesPreparedDestination { prepared.preserve() }
        switch outcome {
        case .committed:
            return .committed(prepared.config)
        case .notPublished(let error):
            return .failed("VM 등록 정보를 저장하지 못했습니다: \(error.localizedDescription)")
        case .publishedButUnsynced(let error):
            return .publishedButUnsynced(prepared.config,
                "VM 파일과 등록 정보는 보존했지만 저장 완료를 확인하지 못했습니다. 라이브러리를 다시 확인하세요: \(error.localizedDescription)")
        }
    }
}
