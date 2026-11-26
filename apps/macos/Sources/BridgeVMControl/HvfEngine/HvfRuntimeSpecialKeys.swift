import SwiftUI

struct HvfRuntimeSpecialKeys: View {
    @ObservedObject var session: HvfEngineSession
    @State private var inputRefused = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) {
                    editingKeys
                    navigationKeys
                    systemKeys
                }
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 12) {
                        editingKeys
                        navigationKeys
                    }
                    systemKeys
                }
                VStack(alignment: .leading, spacing: 6) {
                    editingKeys
                    navigationKeys
                    systemKeys
                }
            }
            if inputRefused {
                Label {
                    Text("Couldn’t send the key to Windows. Check the event log.")
                } icon: {
                    Image(systemName: "exclamationmark.circle").foregroundStyle(.orange)
                }
                    .font(.callout)
                    .accessibilityIdentifier("bridgevm.runtime.specialkeys.refusal")
            }
        }
    }

    private var editingKeys: some View {
        HStack(spacing: 4) {
            Button("Tab") { send("tab") }
            Button("Enter") { send("enter") }
            Button("Space") { send("space") }
            Button("Esc") { send("esc") }
            Button("⌫") { send("backspace") }.accessibilityLabel("Backspace").help("Backspace")
            Button("⌦") { send("delete") }.accessibilityLabel("Delete").help("Delete")
        }
    }

    private var navigationKeys: some View {
        HStack(spacing: 4) {
            Button("←") { send("left") }.accessibilityLabel("Left Arrow")
            Button("↑") { send("up") }.accessibilityLabel("Up Arrow")
            Button("↓") { send("down") }.accessibilityLabel("Down Arrow")
            Button("→") { send("right") }.accessibilityLabel("Right Arrow")
            Button("Home") { send("home") }
            Button("End") { send("end") }
        }
    }

    private var systemKeys: some View {
        HStack(spacing: 4) {
            Button("Win") { send("win") }.accessibilityLabel("Windows key").help("Open the Windows Start menu")
                .accessibilityIdentifier("bridgevm.runtime.specialkeys.windows")
            Button("Ctrl+Alt+Delete") { send("ctrl+alt+delete") }
        }
    }

    private func send(_ action: String) {
        inputRefused = !session.sendToolbarKey(action)
    }
}
