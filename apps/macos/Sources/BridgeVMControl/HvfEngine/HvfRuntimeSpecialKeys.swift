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
                Label("키 입력이 거부되었습니다. 이벤트 기록을 확인하세요.", systemImage: "exclamationmark.circle")
                    .font(.callout).foregroundStyle(.orange)
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
            Button("←") { send("left") }.accessibilityLabel("왼쪽 화살표")
            Button("↑") { send("up") }.accessibilityLabel("위쪽 화살표")
            Button("↓") { send("down") }.accessibilityLabel("아래쪽 화살표")
            Button("→") { send("right") }.accessibilityLabel("오른쪽 화살표")
            Button("Home") { send("home") }
            Button("End") { send("end") }
        }
    }

    private var systemKeys: some View {
        HStack(spacing: 4) {
            Button("Win") { send("win") }.help("Windows 키 (시작 메뉴)")
                .accessibilityIdentifier("bridgevm.runtime.specialkeys.windows")
            Button("Ctrl-Alt-Delete") { send("ctrl+alt+delete") }
        }
    }

    private func send(_ action: String) {
        inputRefused = !session.sendToolbarKey(action)
    }
}
