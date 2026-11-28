import SwiftUI

/// The ordinary stop content stays independent of the explicit destructive escape.
@MainActor
struct HvfForceStopControl<Content: View>: View {
    @ObservedObject var session: HvfEngineSession
    let identifier: String
    let spacing: CGFloat
    private let content: Content
    @State private var capturedTarget: HvfForceStopTarget?
    @State private var confirming = false
    @State private var resultMessage: String?

    init(session: HvfEngineSession, identifier: String, spacing: CGFloat = 4,
         @ViewBuilder content: () -> Content) {
        _session = ObservedObject(wrappedValue: session)
        self.identifier = identifier; self.spacing = spacing; self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: spacing) {
            content
            if let wait = session.stopWait {
                Text(wait.message).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier(identifier + ".stop-wait")
            }
            if session.stopWait == .windowsStillRunning {
                Button("Shut Down Again") { session.askWindowsToShutDownAgain() }
                    .accessibilityIdentifier(identifier + ".ask-again")
            }
            if session.forceStopTarget != nil {
                Button("Force Stop", role: .destructive) {
                    guard let target = session.forceStopTarget else { return }
                    capturedTarget = target; confirming = true
                }
                .accessibilityIdentifier(identifier + ".force-stop")
            }
            if let resultMessage { Text(resultMessage).font(.caption).foregroundStyle(.secondary) }
        }
        .onChange(of: session.connectionState) { _, state in
            if state != .stopping { resultMessage = nil }
        }
        .onChange(of: session.ownedProcessIdentity) { _, _ in resultMessage = nil }
        .confirmationDialog("Force Windows to shut down now?", isPresented: $confirming,
                            titleVisibility: .visible, presenting: capturedTarget) { target in
            Button("Force Stop", role: .destructive) {
                resultMessage = session.requestForceStop(target).message
                capturedTarget = nil
            }
            .accessibilityIdentifier(identifier + ".confirm-force-stop")
            Button("Cancel", role: .cancel) { capturedTarget = nil }
        } message: { _ in
            Text("The VM will stop even if Windows is still closing apps. You may lose unsaved work.")
        }
    }
}

/// A running VM whose drive was unplugged, or that BridgeVM lost contact with: the sentence and its
/// Shut Down or Force Stop over the display. Only the library said so; the VM window, often full screen, went
/// black or froze. Once a stop is under way the stop-wait strip below takes over.
struct HvfRunFailureStrip: View {
    @ObservedObject var session: HvfEngineSession
    @State private var dismissed: String?

    var body: some View {
        if let text = session.displayRunFailureText, dismissed != text {
            HvfNoticeStrip(identifier: "bridgevm.display.failure", title: "VM problem notice",
                           announcement: text, dismiss: { dismissed = text }) {
                HvfForceStopControl(session: session, identifier: "bridgevm.display.failure", spacing: 6) {
                    Label(text, systemImage: "exclamationmark.triangle").font(.callout)
                        .fixedSize(horizontal: false, vertical: true)
                    if session.forceStopTarget == nil, session.terminationStopAvailable {
                        Button("Shut Down") { session.requestTerminationStop() }
                            .accessibilityIdentifier("bridgevm.display.failure.stop")
                    }
                }
            }
        }
    }
}

extension HvfEngineSession {
    /// The supervision failure of a run that is still going, with no stop under way yet.
    var displayRunFailureText: String? {
        guard hasActiveRuntimeWork, stopWait == nil else { return nil }
        return runtimeSupervisionFailureText
    }
}

/// The same control as a notice over the VM display while a Shut Down waits for Windows: the menu's Shut Down is
/// off by then, and a full-screen VM window had no sign of the wait and no Force Stop.
/// Closed, it comes back when the wait changes (Windows still running after the grace).
struct HvfStopWaitStrip: View {
    @ObservedObject var session: HvfEngineSession
    @State private var dismissed = false

    var body: some View {
        Group {
            if let wait = session.stopWait, !dismissed {
                HvfNoticeStrip(identifier: "bridgevm.display.stop", title: "shutdown notice",
                               announcement: wait.message, dismiss: { dismissed = true }) {
                    HvfForceStopControl(session: session, identifier: "bridgevm.display", spacing: 6) { EmptyView() }
                }
            }
        }
        .onChange(of: session.stopWait) { _, _ in dismissed = false }
    }
}
