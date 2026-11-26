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
            Text("Windows will close apps immediately. You may lose unsaved work.")
        }
    }
}
