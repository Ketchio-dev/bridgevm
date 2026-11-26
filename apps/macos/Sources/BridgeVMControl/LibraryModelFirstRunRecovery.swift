import Foundation

extension LibraryModel {
    var firstRunImportBusy: Bool { firstRunImport.isBusy }
    var firstRunImportError: String? { firstRunImport.error }

    func finishFirstRunImport(_ result: FirstRunImportResult, adopt: ((VMConfig) -> Bool)?) -> String? {
        let error: String?
        var published: VMConfig?
        var savedWithNotice = false
        switch result {
        case .committed(let config):
            if let operation = firstRunImport.operationID {
                firstRunImport.advance(.readingLibrary, operation: operation)
            }
            let adopted = adopt?(config) ?? add(config)
            if adopted { proMode = false }
            error = adopted ? HvfWindowsImportNotice.read(bundlePath: config.bundlePath) : "The VM was saved, but it did not appear in the library. Its files are safe."
            if adopted, error != nil {
                savedWithNotice = true
                selectedID = Self.firstRunImportSelectionID
            }
            published = error == nil ? nil : config
        case .publishedButUnsynced(let config, let message):
            published = config
            error = message
        case .failed(let message):
            error = message
        }
        firstRunImport.finish(error: error, publishedConfig: published, savedWithNotice: savedWithNotice)
        finishVMPreparation(succeeded: error == nil)
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
        guard !firstRunImportBusy else { return "Checking the library…" }
        guard let expected = firstRunImport.publishedConfig else { return firstRunImportError }
        guard !Task.isCancelled else { return firstRunImportError }
        firstRunImport.beginRecovery()
        defer { finishVMPreparation(succeeded: firstRunImportError == nil) }
        let root = rootURL
        let persisted = await Task.detached(priority: .userInitiated) {
            readback(expected, root)
        }.value
        guard let persisted, add(persisted) else {
            firstRunImport.recoveryFailed(
                "The saved VM could not be opened. Check the library folder and choose Open Saved VM again.")
            return firstRunImportError
        }
        proMode = false
        firstRunImport.finish(error: nil)
        return nil
    }
}
