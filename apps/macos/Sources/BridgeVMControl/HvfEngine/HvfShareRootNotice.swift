import Combine

/// Informational log-derived feedback; it never authorizes a filesystem action.
@MainActor
final class HvfShareRootNotice: ObservableObject {
    @Published private(set) var isBlocked = false
    @Published private(set) var launchUnavailable = false
    @Published private(set) var stopped: HvfShareStop?
    /// The runtime holds every change while the guest folder's listing is over its limit.
    @Published private(set) var paused: HvfSharePaused?
    /// Files the runtime left out for being over the size limit, this run.
    @Published private(set) var tooLarge: HvfShareTooLarge?
    /// Windows files left on the guest because the Mac had too little space for them, until they
    /// arrive.
    @Published private(set) var lowSpace: HvfShareWaitingFiles?
    /// Why the Mac refused the last file from Windows, until one arrives again.
    @Published private(set) var writeRefused: HvfShareWriteRefused?
    /// Mac files Windows refused, which the runtime sends again, until they reach Windows.
    @Published private(set) var putFailed: HvfShareWaitingFiles?
    /// The last Mac names reported as ones Windows can't keep apart, until one of them syncs.
    @Published private(set) var aliased: [String]?
    /// The shared folder's name when its Mac folder was missing at Start: the VM started with the share,
    /// and the runtime shares the folder once it appears (`root-state ready`). A guest restart clears it;
    /// the next boot reports a still missing folder as blocked.
    @Published private(set) var missingFolder: String?
    @Published private(set) var lastRecoveredCopy: String?

    func consume(_ event: BvAgentEvent) {
        // A whole file's arrival; a large one logs this line too.
        if case let .shareEvent(.guestToHost, name, _, _) = event { synced(name); return arrived(name) }
        // A file skipped under the lower limit before a Guest Tools update syncs after it.
        if case let .shareEvent(.hostToGuest, name, _, _) = event {
            synced(name)
            if let putFailed, putFailed.isWaiting(for: name) { self.putFailed = putFailed.arrived(name) }
            return
        }
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        switch line {
        case HvfEngineConfig.shareUnavailableMessage: launchUnavailable = true; missingFolder = nil
        case _ where HvfGuestResetRecord.matches(line): reset()
        case "BVAGENT SHARE root-state blocked": update(true)
        case "BVAGENT SHARE root-state ready":
            update(false)
            if missingFolder != nil { missingFolder = nil }
        case "BVAGENT SHARE resumed": if paused != nil { paused = nil }
        default:
            if let copy = Self.recoveredCopy(in: line) {
                lastRecoveredCopy = copy
            } else if stopped == nil, let stop = HvfShareStop(line: line) {
                stopped = stop
            } else if let pause = HvfSharePaused(line: line) {
                if paused != pause { paused = pause }
            } else if let skip = HvfShareTooLarge.skip(in: line) {
                tooLarge = HvfShareTooLarge(count: (tooLarge?.count ?? 0) + 1, last: skip.name,
                                            limit: skip.limit, fromWindows: skip.fromWindows)
            } else if let refusal = HvfShareWriteRefused(line: line) {
                if writeRefused != refusal { writeRefused = refusal }
            } else if let name = HvfShareWaitingFiles.lowSpaceName(in: line) {
                lowSpace = lowSpace?.reported(name) ?? HvfShareWaitingFiles(name)
            } else if let name = HvfShareWaitingFiles.putErrorName(in: line) {
                putFailed = putFailed?.reported(name) ?? HvfShareWaitingFiles(name)
            } else if let names = HvfShareAlias.names(in: line) {
                if aliased != names { aliased = names }
            } else if let file = BvAgentEvent.shareFile(in: line, after: "BVAGENT SHARE transfer-done guest->host ") {
                arrived(file.name)
            }
        }
    }

    /// The host reports both leaf names after publishing the copy beside the original.
    private static func recoveredCopy(in line: String) -> String? {
        let prefix = "BVAGENT SHARE conflict kept-mac="
        guard line.hasPrefix(prefix) else { return nil }
        let names = line.dropFirst(prefix.count)
        guard !names.contains("/"), !names.contains("\\"), !names.contains(where: \.isNewline),
              !names.unicodeScalars.contains(where: { $0.properties.generalCategory == .control || $0.properties.isBidiControl })
        else { return nil }

        // A literal field separator in the original name also occurs in the copy's name.
        // The host only inserts " (Windows copy...)" before the extension, so the middle
        // separator always divides the two names.
        let separator = " windows-copy="
        let pieces = names.components(separatedBy: separator)
        guard pieces.count.isMultiple(of: 2) else { return nil }
        let half = pieces.count / 2
        guard !pieces.prefix(half).joined(separator: separator).isEmpty else { return nil }
        let copy = pieces.suffix(half).joined(separator: separator)
        return copy.isEmpty ? nil : copy
    }

    /// The file the too-large notice names was synced after all (the limit rose), or one of the names
    /// Windows couldn't keep apart was (a rename ended the hold), so that notice is stale.
    private func synced(_ name: String) {
        if tooLarge?.last == name { tooLarge = nil }
        if aliased?.contains(name) == true { aliased = nil }
    }

    /// `name` reached the Mac. Every synced file says so: only a waiting one changes the notice.
    private func arrived(_ name: String) {
        if writeRefused != nil { writeRefused = nil }
        guard let lowSpace, lowSpace.isWaiting(for: name) else { return }
        self.lowSpace = lowSpace.arrived(name)
    }

    /// A new run or boot generation; at Start, `missingFolder` names a shared folder missing then.
    func reset(missingFolder folder: String? = nil) {
        if launchUnavailable { launchUnavailable = false }
        if missingFolder != folder { missingFolder = folder }
        update(false)
        if stopped != nil { stopped = nil }
        if paused != nil { paused = nil }
        if tooLarge != nil { tooLarge = nil }
        if lastRecoveredCopy != nil { lastRecoveredCopy = nil }
        if lowSpace != nil { lowSpace = nil }
        if writeRefused != nil { writeRefused = nil }
        if putFailed != nil { putFailed = nil }
        if aliased != nil { aliased = nil }
    }

    private func update(_ blocked: Bool) {
        guard isBlocked != blocked else { return }
        isBlocked = blocked
    }
}
