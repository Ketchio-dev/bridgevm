import Combine
import SwiftUI

/// The firmware found nothing to start on the VM's disk (`BVBOOT no-bootable-disk`, the probe's
/// host_support.rs), so the window shows only its own lines and Windows never comes. With
/// `loader=other-cpu` it refused Windows' boot loader as built for another processor: a disk from
/// an Intel or AMD PC. Until dismissed or the next run.
@MainActor
final class HvfBootFailureNotice: ObservableObject {
    @Published private(set) var text: String?
    /// The user closed the strip; `text` stays, so other notices still know.
    @Published private(set) var dismissed = false

    nonisolated static func text(for line: String) -> String? {
        guard line.hasPrefix("BVBOOT no-bootable-disk") else { return nil }
        return line.contains("loader=other-cpu")
            ? "Windows can’t start because this disk has Windows for Intel or AMD PCs. Create a VM with Windows 11 for Arm instead."
            : "Windows can’t start because this VM’s disk has nothing to start. If you imported the disk, check that it has Windows 11 for Arm on it."
    }

    func consume(_ event: BvAgentEvent) {
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        if let text = Self.text(for: line), text != self.text { self.text = text }
    }

    func dismiss() { dismissed = true }
    func reset() {
        if text != nil { text = nil }
        if dismissed { dismissed = false }
    }
}

struct HvfBootFailureNoticeView: View {
    @ObservedObject var notice: HvfBootFailureNotice

    var body: some View {
        if let text = notice.text, !notice.dismissed {
            HvfNoticeStrip(identifier: "bridgevm.runtime.boot-failure", dismiss: { notice.dismiss() }) {
                Text(text)
            }
        }
    }
}
