import SwiftUI

/// Over the display while keyboard and mouse are basic: a key or click Windows never confirmed (a receipt deadline,
/// a failed send, a rejected or partly inserted request) ends the agent's route, which is never asked to repeat it,
/// and input goes through the VMM's keyboard and mouse until the agent or Windows restarts. Those type characters at
/// US key positions (xhci_hid_input/setup_input/actions.rs), take other text only through the Windows clipboard, have
/// no middle button and scroll in whole vertical notches.
struct HvfInputStoppedStrip: View {
    @ObservedObject var session: HvfEngineSession
    @State private var dismissed = false

    static let text = "Windows stopped confirming input, so BridgeVM switched to basic keyboard and mouse, which types as a US keyboard; restart Windows to get full input back."

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
