import Foundation

extension HvfEngineSession {
    /// A Finder drop onto the display: stage the items next to the control file, then hand the
    /// folder to the runtime, which copies it to the guest user's Desktop (file_drop.rs); with
    /// `open` (Finder's Open With, a drop on the Dock icon, HvfOpenFiles) it then opens each item
    /// there with its default Windows app. While Windows is still starting, the staged items wait for
    /// the agent's SERVICE start. Returns the staging work so tests can wait for it.
    @discardableResult
    func copyDroppedFiles(_ items: [URL], open: Bool = false) -> Task<Void, Never>? {
        guard !items.isEmpty else { return nil }
        switch connectionState {
        case .booting, .connected, .timedOut: break
        case .stopped, .stopping:
            fileDrops.refuse(HvfFileDropQueue.notRunningText)
            return nil
        }
        let root = URL(fileURLWithPath: (config.ctlFilePath as NSString).deletingLastPathComponent)
            .appendingPathComponent("drops", isDirectory: true)
        let itemName = items.count == 1 ? items[0].lastPathComponent : nil
        let (id, staging) = fileDrops.begin(in: root, open: open, itemName: itemName)
        return Task { [weak self] in
            let outcome = await Task.detached(priority: .userInitiated) {
                Result(catching: { try HvfFileDropStaging.stage(items, into: staging) })
            }.value
            self?.finishStaging(id: id, staging: staging, open: open, outcome: outcome)
        }
    }

    private func finishStaging(id: String, staging: URL, open: Bool, outcome: Result<HvfFileDropStaging.Summary, Error>) {
        // The VM stopped while this drop was staging: never hand it to a later run.
        guard fileDrops.isPending(id) else {
            try? FileManager.default.removeItem(at: staging)
            return
        }
        switch outcome {
        case let .failure(error):
            append(.unknown("file drop \(id) staging failed: \(error.localizedDescription)"))
            fileDrops.stagingFailed(id)
        case let .success(summary):
            // A clone is instant and takes no space; a plain copy (another volume) reads every byte.
            append(.unknown("file drop \(id) staged: files=\(summary.files) folders=\(summary.folders) "
                + "bytes=\(summary.bytes) cloned=\(summary.cloned) copied=\(summary.copied) skipped=\(summary.skipped) unreadable=\(summary.unreadable)"))
            guard summary.files + summary.folders > 0 else { return fileDrops.nothingToCopy(id, unreadable: summary.unreadable > 0) }
            // The runtime reads the control file only once the agent's service started.
            guard serviceStarted, case .connected = connectionState else { return fileDrops.waitForWindows(id, summary) }
            fileDrops.staged(id, summary, sent: sendDrop(id, staging: staging, open: open))
        }
    }

    /// At the agent's SERVICE start: the drops staged while Windows was starting go now, in drop order.
    func sendDropsWaitingForWindows() {
        for job in fileDrops.waitingForWindows() {
            fileDrops.staged(job.id, job.summary, sent: sendDrop(job.id, staging: job.staging, open: job.open))
        }
    }

    private func sendDrop(_ id: String, staging: URL, open: Bool) -> Bool {
        sendCtl("BVDROP \(id) \(Data(staging.path.utf8).base64EncodedString())" + (open ? " open" : ""))
    }
}
