import SwiftUI

/// The Windows display resolution the user chose; nil leaves it to the VMM.
struct HvfGuestResolutionPicker: View {
    static let resolutions = ["1024x768", "1280x800", "1440x900", "1680x1050",
                              "1920x1080", "1920x1200", "2560x1440", "2560x1600"]

    @ObservedObject var session: HvfEngineSession
    @Binding var selection: String?
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Picker("Display resolution", selection: Binding(get: { selection }, set: update)) {
            Text("Automatic (1280x800)").tag(String?.none)
            ForEach(Self.resolutions, id: \.self) { Text($0).tag(Optional($0)) }
        }
        .fixedSize()
        .accessibilityIdentifier("bridgevm.runtime.guest-resolution")
        .disabled(isDisabled)
        .help("Windows 화면 해상도입니다. 다음 시작부터 적용되며, Windows가 이전 해상도를 기억하고 있으면 BridgeVM이 시작 후 자동으로 맞춥니다. 창 크기를 바꿔도 해상도는 바뀌지 않습니다."
            + (session.persistsLibraryPreferences ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : ""))
        .alert("화면 해상도 설정", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("확인") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    private func update(_ value: String?) {
        let result = session.applyGuestResolution(value)
        selection = result.value
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the picker shows afterwards: vm.json's value for a library VM, plus any message.
    func applyGuestResolution(_ value: String?,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (value: String?, message: String?) {
        guard persistsLibraryPreferences else { return (value, nil) }
        let message = saveLibraryPreference("화면 해상도 설정", publish: publish) {
            $0.guestResolution = value
        }
        return (config.libraryContext?.config.guestResolution, message)
    }
}
