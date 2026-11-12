import SwiftUI

struct HvfStartCloseSettings: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var power = ControlAppPower.shared
    @AppStorage(ControlDisplayClose.preferenceKey) private var closeChoice = ""
    @State private var message: String?
    var body: some View {
        Section("시작과 닫기") {
            Toggle("Windows 실행 중 Mac 잠자기 방지", isOn: Binding(
                get: { power.enabled }, set: { power.setEnabled($0) }))
            Text("Windows가 실행되는 동안 Mac은 잠자지 않으며 화면은 꺼질 수 있습니다.")
                .font(.caption).foregroundStyle(.secondary)
            Picker("창을 닫을 때", selection: $closeChoice) {
                Text("물어보기").tag("")
                ForEach([ControlDisplayCloseChoice.background, .pause, .shutdown], id: \.rawValue) {
                    Text($0.title).tag($0.rawValue)
                }
            }
            if session.persistsLibraryPreferences {
                Toggle("BridgeVM을 열 때 자동 시작", isOn: Binding(
                    get: { session.config.libraryContext?.config.startWithBridgeVM == true },
                    set: { value in
                        message = session.saveLibraryPreference("자동 시작 설정",
                            publish: { VMRegistrationWriter.commit($0, to: $1) }) { $0.startWithBridgeVM = value }
                    }))
                    .disabled(!session.libraryPreferencesEditable)
                Text("BridgeVM을 열 때 Option 키를 누르고 있으면 자동 시작을 건너뜁니다.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let message { Text(message).foregroundStyle(.secondary) }
        }
    }
}
