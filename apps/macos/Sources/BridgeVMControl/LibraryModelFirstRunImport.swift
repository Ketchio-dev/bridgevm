import Foundation

extension LibraryModel {
    /// Accepted transactions outlive cancellation of their waiting UI task.
    @discardableResult
    func importExistingHvfVM(
        _ inputs: FirstRunImport.Inputs,
        snapshotHelper: URL = HvfMediaImportHelper.bundled,
        worker: @escaping FirstRunImportOperation = {
            FirstRunImportWorker.run($0, libraryRoot: $1, snapshotHelper: $2, progress: $3)
        },
        adopt: ((VMConfig) -> Bool)? = nil
    ) async -> String? {
        guard !firstRunImportBusy else { return "VM 가져오기가 이미 진행 중입니다." }
        guard firstRunImport.publishedConfig == nil else { return firstRunImportError }
        guard !Task.isCancelled else { return "VM 가져오기를 시작하지 않았습니다." }
        let operation = firstRunImport.begin()
        let root = rootURL
        let result = await Task.detached(priority: .userInitiated) {
            worker(inputs, root, snapshotHelper) { stage in
                Task { @MainActor in self.firstRunImport.advance(stage, operation: operation) }
            }
        }.value
        return finishFirstRunImport(result, adopt: adopt)
    }
}
