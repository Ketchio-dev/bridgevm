import SwiftUI

struct HvfRuntimeSpecialKeys: View {
    @ObservedObject var session: HvfEngineSession
    @State private var inputRefused = false

    var body: some View {
        VStack(alignment: .leading) {
            HStack(spacing: 8) {
                Button("Esc") { send("esc") }
                Button("⌫") { send("backspace") }.help("Backspace")
                Button("⌦") { send("delete") }.help("Delete")
                Divider().frame(height: 18)
                Button("←") { send("left") }
                Button("↑") { send("up") }
                Button("↓") { send("down") }
                Button("→") { send("right") }
                Button("Home") { send("home") }
                Button("End") { send("end") }
                Spacer()
                Button("Ctrl-Alt-Delete") { send("ctrl+alt+delete") }
            }
            if inputRefused {
                Label("Key input refused. Check the event log.", systemImage: "exclamationmark.circle")
                    .font(.callout).foregroundStyle(.orange)
                    .accessibilityIdentifier("bridgevm.runtime.specialkeys.refusal")
            }
        }
    }

    private func send(_ action: String) {
        inputRefused = !session.sendToolbarKey(action)
    }
}
