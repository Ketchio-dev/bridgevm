import Foundation

extension LibraryModel {
    var firstRunImportBusy: Bool { firstRunImport.isBusy }
    var firstRunImportError: String? { firstRunImport.error }

    func finishFirstRunImport(_ result: FirstRunImportResult, adopt: ((VMConfig) -> Bool)?) -> String? {
        let error: String?
        var published: VMConfig?
        switch result {
        case .committed(let config):
            if let operation = firstRunImport.operationID {
                firstRunImport.advance(.readingLibrary, operation: operation)
            }
            let adopted = adopt?(config) ?? add(config)
            if adopted { proMode = false }
            error = adopted ? nil : "VM은 저장했지만 라이브러리에서 찾지 못했습니다. 저장된 파일은 보존했습니다."
            published = adopted ? nil : config
        case .publishedButUnsynced(let config, let message):
            published = config
            error = message
        case .failed(let message):
            error = message
        }
        firstRunImport.finish(error: error, publishedConfig: published)
        return error
    }

    /// Read-back confirms the registration can be loaded; it does not retry or
    /// claim the earlier directory sync. No source media or copying is involved.
    @discardableResult
    func recoverPublishedFirstRunImport(
        readback: @escaping @Sendable (VMConfig, URL) -> VMConfig? = { expected, root in
            VMLibrary.list(rootURL: root).first {
                $0.slug == expected.slug && $0.bundlePath == expected.bundlePath
            }
        }
    ) async -> String? {
        guard !firstRunImportBusy else { return "라이브러리를 확인하고 있습니다." }
        guard let expected = firstRunImport.publishedConfig else { return firstRunImportError }
        guard !Task.isCancelled else { return firstRunImportError }
        firstRunImport.beginRecovery()
        let root = rootURL
        let persisted = await Task.detached(priority: .userInitiated) {
            readback(expected, root)
        }.value
        guard let persisted, add(persisted) else {
            firstRunImport.recoveryFailed(
                "저장된 VM을 아직 불러올 수 없습니다. 라이브러리 폴더를 확인한 뒤 ‘저장된 VM 불러오기’를 다시 누르세요.")
            return firstRunImportError
        }
        proMode = false
        firstRunImport.finish(error: nil)
        return nil
    }
}
