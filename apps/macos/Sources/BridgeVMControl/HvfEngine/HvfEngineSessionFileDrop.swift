import Foundation

extension HvfEngineSession {
    /// A Finder drop onto the display: stage the items next to the control file, then hand the
    /// folder to the runtime, which copies it to the guest user's Desktop (file_drop.rs). Returns
    /// the staging work so tests can wait for it.
    @discardableResult
    func copyDroppedFiles(_ items: [URL]) -> Task<Void, Never>? {
        guard !items.isEmpty else { return nil }
        guard serviceStarted, case .connected = connectionState else {
            fileDrops.refuse(HvfFileDropQueue.notReadyText)
            return nil
        }
        let root = URL(fileURLWithPath: (config.ctlFilePath as NSString).deletingLastPathComponent)
            .appendingPathComponent("drops", isDirectory: true)
        let (id, staging) = fileDrops.begin(in: root)
        return Task { [weak self] in
            let outcome = await Task.detached(priority: .userInitiated) {
                Result(catching: { try HvfFileDropStaging.stage(items, into: staging) })
            }.value
            self?.finishStaging(id: id, staging: staging, outcome: outcome)
        }
    }

    private func finishStaging(id: String, staging: URL, outcome: Result<HvfFileDropStaging.Summary, Error>) {
        switch outcome {
        case let .failure(error):
            append(.unknown("file drop \(id) staging failed: \(error.localizedDescription)"))
            fileDrops.stagingFailed(id)
        case let .success(summary):
            // A clone is instant and takes no space; a plain copy (another volume) reads every byte.
            append(.unknown("file drop \(id) staged: files=\(summary.files) folders=\(summary.folders) "
                + "bytes=\(summary.bytes) cloned=\(summary.cloned) copied=\(summary.copied) skipped=\(summary.skipped)"))
            guard summary.files + summary.folders > 0 else { return fileDrops.nothingToCopy(id) }
            let sent = sendCtl("BVDROP \(id) \(Data(staging.path.utf8).base64EncodedString())")
            fileDrops.staged(id, summary, sent: sent)
        }
    }
}
