import SwiftUI

/// Over the display while keyboard and mouse refuse input: a key or click Windows never confirmed
/// (a receipt deadline, a failed send or a rejected request) stops both routes, so they cannot repeat or
/// reorder it, until the agent restarts or Windows restarts. Keys only beeped and clicks did nothing.
struct HvfInputStoppedStrip: View {
    @ObservedObject var session: HvfEngineSession
    @State private var dismissed = false

    static let text = "Keyboard and mouse stopped because Windows didn’t confirm your input in time. Choose Virtual Machine > Restart to use them again."

    var body: some View {
        if session.inputStopped, !dismissed {
            HvfNoticeStrip(identifier: "bridgevm.display.input-stopped", dismiss: { dismissed = true }) {
                Label {
                    Text(Self.text).fixedSize(horizontal: false, vertical: true)
                } icon: {
                    Image(systemName: "keyboard.badge.exclamationmark").foregroundStyle(.orange)
                }
            }
            .onChange(of: session.inputStopped) { _, stopped in if !stopped { dismissed = false } }
        }
    }
}
