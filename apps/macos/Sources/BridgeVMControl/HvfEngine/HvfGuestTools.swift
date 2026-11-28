import Foundation
import SwiftUI

enum HvfGuestTools {
    static let missing = "Choose Install Guest Tools to enable clipboard sharing and shared folders."
    /// BVTOOLS is the drive's label in Windows (scripts/build-guest-tools-volume.sh).
    static let nextStart = "Shut down Windows, start it again, then run install.cmd from the BVTOOLS drive."
    static let attached = "Run install.cmd from the BVTOOLS drive in Windows and approve the administrator prompt."
    static let failed = "Check the free space on your Mac, then choose Install Guest Tools again."

    static func image(_ config: HvfEngineConfig) -> URL? {
        config.libraryContext.map { URL(fileURLWithPath: $0.config.bundlePath).appendingPathComponent("metadata/guest-tools.raw") }
    }

    static func attachment(_ config: HvfEngineConfig) -> String? {
        guard let image = image(config), FileManager.default.fileExists(atPath: image.path),
              !FileManager.default.fileExists(atPath: config.evidenceDir + "/" + VMLibrary.agentSeenMarkerName) else { return nil }
        return image.path
    }

    static func build(repoRoot: URL, image: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [repoRoot.appendingPathComponent("scripts/build-guest-tools-volume.sh").path,
            repoRoot.appendingPathComponent("scripts/win-assets").path, image.path]
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw CocoaError(.fileWriteUnknown) }
    }
}

extension HvfEngineSession {
    /// Offered while the agent is missing, and also once an agent that ran here can't connect (an antivirus
    /// took a file, the task was deleted): only a reinstall brings that back.
    var offersGuestToolsInstall: Bool {
        !serviceStarted && (agentSilenceNotice.shown == .missingTools || agentSilenceNotice.shown == .notConnected
            || guestToolsMessage != nil)
    }

    func prepareGuestTools() {
        guard !guestToolsPreparing, let image = HvfGuestTools.image(config) else { return }
        guestToolsPreparing = true
        // The drive is attached only while no agent was seen here (attachment): a reinstall brings it back,
        // and the next SERVICE start writes the marker again.
        try? FileManager.default.removeItem(at: agentSeenMarker)
        let root = repoRoot
        Task {
            do {
                if !FileManager.default.fileExists(atPath: image.path) {
                    try await Task.detached { try HvfGuestTools.build(repoRoot: root, image: image) }.value
                }
                guestToolsMessage = connectionState == .stopped ? HvfGuestTools.attached : HvfGuestTools.nextStart
            } catch { guestToolsMessage = HvfGuestTools.failed }
            guestToolsPreparing = false
        }
    }
}

struct HvfGuestToolsStrip: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var notice: HvfAgentSilenceNotice
    @State private var dismissed = false

    init(session: HvfEngineSession) {
        self.session = session
        self.notice = session.agentSilenceNotice
    }

    var body: some View {
        let needsTools = !session.serviceStarted && (notice.shown == .missingTools || session.guestToolsMessage != nil)
        Group {
            if needsTools && !dismissed {
                HvfNoticeStrip(identifier: "bridgevm.display.guest-tools", title: "Guest Tools notice",
                               announcement: session.guestToolsMessage ?? HvfGuestTools.missing, dismiss: { dismissed = true }) {
                    VStack(alignment: .leading, spacing: 6) {
                        if session.guestToolsMessage == nil {
                            Text(HvfGuestTools.missing).fixedSize(horizontal: false, vertical: true)
                        }
                        HvfGuestToolsAction(session: session)
                    }
                }
            }
        }
        .onChange(of: needsTools) { _, needed in
            if !needed { dismissed = false }
        }
    }
}

struct HvfGuestToolsAction: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var notice: HvfAgentSilenceNotice

    init(session: HvfEngineSession) {
        self.session = session
        self.notice = session.agentSilenceNotice
    }

    var body: some View {
        if session.offersGuestToolsInstall {
            VStack(alignment: .leading, spacing: 6) {
                if let text = session.guestToolsMessage { Text(text).font(.callout) }
                Button("Install Guest Tools", action: session.prepareGuestTools)
                    .disabled(session.guestToolsPreparing)
                    .accessibilityIdentifier("bridgevm.runtime.guest-tools.install")
            }
        }
    }
}
