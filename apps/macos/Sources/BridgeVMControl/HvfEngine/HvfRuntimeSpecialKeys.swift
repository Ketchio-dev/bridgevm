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
                Button("Win") { send("win") }.help("Windows 키 (시작 메뉴)")
                    .accessibilityIdentifier("bridgevm.runtime.specialkeys.windows")
                Button("Ctrl-Alt-Delete") { send("ctrl+alt+delete") }
            }
            if inputRefused {
                Label("키 입력이 거부되었습니다. 이벤트 기록을 확인하세요.", systemImage: "exclamationmark.circle")
                    .font(.callout).foregroundStyle(.orange)
                    .accessibilityIdentifier("bridgevm.runtime.specialkeys.refusal")
            }
        }
    }

    private func send(_ action: String) {
        inputRefused = !session.sendToolbarKey(action)
    }
}
