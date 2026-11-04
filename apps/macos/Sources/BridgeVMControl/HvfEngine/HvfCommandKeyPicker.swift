import SwiftUI

/// The per-VM 'Command 키' setting (HvfCommandKeyMode).
struct HvfCommandKeyPicker: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var selection: HvfCommandKeyMode
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Picker("Command 키", selection: Binding(get: { selection }, set: update)) {
            Text("Mac처럼").tag(HvfCommandKeyMode.mac)
            Text("Windows 키로").tag(HvfCommandKeyMode.windows)
        }
        .fixedSize()
        .accessibilityIdentifier("bridgevm.runtime.command-key")
        .disabled(isDisabled)
        .help("Mac처럼(기본): ⌘C, ⌘V처럼 Command 조합은 Windows에서 Ctrl 조합이 되고, ⌘←/→는 줄의 처음/끝, ⌘↑/↓는 문서의 처음/끝, ⌥←/→는 단어 이동, ⌥⌫는 단어 지우기로 Mac과 같이 움직입니다(⇧를 더하면 선택). 왼쪽 Command만 눌렀다 떼면 Windows 키(시작 메뉴)입니다(오른쪽 Command는 한/영 키로 쓰는 경우가 많아 제외). "
            + "Windows 키로: Command가 모든 조합에서 Windows 키입니다(⌘E는 파일 탐색기). 두 방식 모두 그 밖의 Option 조합은 Alt이고, ⌘Q, ⌘H, ⌘M, ⌘쉼표, ⌃⌘F와 ⌘Tab은 Mac이 사용합니다."
            + (session.persistsLibraryPreferences ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : ""))
        .alert("Command 키 설정", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("확인") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    private func update(_ mode: HvfCommandKeyMode) {
        let result = session.applyCommandKeyMode(mode)
        selection = result.mode
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the picker shows afterwards: vm.json's value for a library VM, plus any message.
    func applyCommandKeyMode(_ mode: HvfCommandKeyMode,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (mode: HvfCommandKeyMode, message: String?) {
        guard persistsLibraryPreferences else { return (mode, nil) }
        let message = saveLibraryPreference("Command 키 설정", publish: publish) {
            $0.commandKeyMode = mode == .mac ? nil : mode.rawValue
        }
        return (HvfCommandKeyMode(saved: config.libraryContext?.config.commandKeyMode), message)
    }
}
