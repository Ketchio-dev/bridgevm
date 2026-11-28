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
        guard !firstRunImportBusy else { return "An import is already in progress." }
        guard firstRunImport.publishedConfig == nil else { return firstRunImportError }
        guard !Task.isCancelled else { return "The import did not start." }
        let operation = firstRunImport.begin(displayName: inputs.displayName)
        let cancel = firstRunImport.cancelHandle
        let root = rootURL
        let result = await Task.detached(priority: .userInitiated) {
            HvfHelperProcess.$cancelHandle.withValue(cancel) {
                worker(inputs, root, snapshotHelper) { stage in
                    Task { @MainActor in self.firstRunImport.advance(stage, operation: operation) }
                }
            }
        }.value
        return finishFirstRunImport(result, adopt: adopt)
    }
}
