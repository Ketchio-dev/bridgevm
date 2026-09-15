import Foundation

extension LibraryModel {
    /// Once accepted, the detached transaction completes even if its waiting UI
    /// task is cancelled. It alone decides rollback; adoption never deletes files.
    @discardableResult
    func importExistingHvfVM(
        _ inputs: FirstRunImport.Inputs,
        snapshotHelper: URL = HvfMediaImportHelper.bundled,
        worker: @escaping @Sendable (FirstRunImport.Inputs, URL, URL) -> FirstRunImportResult = {
            FirstRunImportWorker.run($0, libraryRoot: $1, snapshotHelper: $2)
        },
        adopt: ((VMConfig) -> Bool)? = nil
    ) async -> String? {
        guard !firstRunImportBusy else { return "VM 가져오기가 이미 진행 중입니다." }
        guard !Task.isCancelled else { return "VM 가져오기를 시작하지 않았습니다." }
        firstRunImportBusy = true
        firstRunImportError = nil
        defer { firstRunImportBusy = false }
        let root = rootURL
        let result = await Task.detached(priority: .userInitiated) {
            worker(inputs, root, snapshotHelper)
        }.value
        let error: String?
        switch result {
        case .committed(let config):
            let adopted = adopt?(config) ?? add(config)
            error = adopted ? nil : "VM은 저장했지만 라이브러리에서 찾지 못했습니다. 저장된 파일은 보존했습니다."
        case .failed(let message), .publishedButUnsynced(_, let message):
            error = message
        }
        firstRunImportError = error
        return error
    }
}
