import Foundation
import SwiftUI

/// Windows pauses only once a write finds the drive with its disk full (HvfGuestPause); this warns
/// before. The session's poll reads that drive's free space at most every `interval` while Windows
/// runs. Below `threshold` one dismissible sentence shows, once per run, gone when space comes back.
@MainActor
final class HvfLowSpaceNotice: ObservableObject {
    static let threshold: Int64 = 5_000_000_000
    static let interval: TimeInterval = 30
    @Published private(set) var text: String?
    private var shown = false
    private var checkedAt: TimeInterval?

    /// `available` is HvfImportSpace.available but in tests; `disk` is only resolved when a check is due.
    func refresh(now: TimeInterval, running: Bool, disk: () -> URL, available: (URL) -> Int64?) {
        guard running, checkedAt.map({ now - $0 >= Self.interval }) ?? true else { return }
        checkedAt = now
        let disk = disk()
        guard let free = available(disk) else { return }
        if free >= Self.threshold {
            if text != nil { text = nil }
        } else if !shown || text != nil { // dismissed, or space came back: not again this run
            shown = true
            text = Self.text(free: free, drive: HvfVMDrive.name(of: disk))
        }
    }

    func dismiss() { text = nil }
    func reset() { text = nil; shown = false; checkedAt = nil }

    static func text(free: Int64, drive: String?) -> String {
        let size = ByteCountFormatter.string(fromByteCount: free, countStyle: .file)
        let place = drive.map { "“\($0)”" } ?? "your Mac"
        return "Only \(size) is left on \(place). Windows pauses if it fills up, so free up space or delete snapshots."
    }
}

struct HvfLowSpaceNoticeView: View {
    @ObservedObject var notice: HvfLowSpaceNotice

    var body: some View {
        if let text = notice.text {
            HvfNoticeStrip(identifier: "bridgevm.storage.low-space", title: "low disk space notice",
                           announcement: text, dismiss: { notice.dismiss() }) {
                Text(text)
            }
        }
    }
}

extension HvfEngineSession {
    /// From poll(): the low-space notice for the drive with this run's disk, while Windows runs.
    func refreshLowSpace() {
        let running: Bool
        switch connectionState {
        case .booting, .connected: running = pause == .running && !driveDisconnected
        case .stopped, .stopping, .timedOut: running = false
        }
        lowSpaceNotice.refresh(now: ownedStartNow(), running: running,
                               disk: { URL(fileURLWithPath: config.targetDiskPath).resolvingSymlinksInPath() },
                               available: freeSpace)
    }
}
