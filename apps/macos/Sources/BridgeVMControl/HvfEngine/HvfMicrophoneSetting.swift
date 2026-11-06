import AVFoundation
import SwiftUI

/// macOS's answer to this app's microphone access, as the setting needs it.
enum HvfMicrophoneStatus: Equatable {
    case authorized, undetermined, denied
}

/// macOS's microphone permission for this app, behind a protocol so that no test
/// reaches it: only the user turning the setting on may make macOS ask.
protocol HvfMicrophonePermission {
    var status: HvfMicrophoneStatus { get }
    /// Lets macOS ask the user; `completion` runs on the main actor.
    func requestAccess(_ completion: @escaping @MainActor (Bool) -> Void)
}

struct HvfSystemMicrophonePermission: HvfMicrophonePermission {
    var status: HvfMicrophoneStatus {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return .authorized
        case .notDetermined: return .undetermined
        default: return .denied // denied or restricted
        }
    }

    func requestAccess(_ completion: @escaping @MainActor (Bool) -> Void) {
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            Task { @MainActor in completion(granted) }
        }
    }
}

/// Why a VM whose microphone setting is on still records silence. The VM starts either way.
enum HvfMicrophoneNotice {
    static let denied = "macOS가 마이크 사용을 허용하지 않아 이 VM의 Windows에는 마이크가 없습니다. 시스템 설정 > 개인정보 보호 및 보안 > 마이크에서 BridgeVM Control을 허용한 뒤 VM을 다시 시작하세요."
    static let undetermined = "마이크 권한이 없어 이 VM의 Windows에는 마이크가 없습니다. VM을 중지한 뒤 '마이크 사용'을 껐다가 다시 켜면 macOS가 권한을 묻습니다."

    static func text(_ status: HvfMicrophoneStatus) -> String? {
        switch status {
        case .authorized: return nil
        case .denied: return denied
        case .undetermined: return undetermined
        }
    }
}

/// The guest microphone. Off (the default), Windows has no microphone; on, it gets one that
/// records the Mac's default input while Windows captures, once macOS allows it. The launch
/// reads the setting, so a change applies at the next start.
struct HvfMicrophoneToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    var permission: any HvfMicrophonePermission = HvfSystemMicrophonePermission()
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("마이크 사용", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.microphone")
            .disabled(isDisabled)
            .help("켜면 Windows에 마이크가 생기고, Windows가 녹음하는 동안 Mac의 기본 입력 장치를 씁니다. 끄면(기본) Windows에는 마이크가 없습니다. 바꾼 설정은 다음 VM 시작부터 적용됩니다."
                + (session.persistsLibraryPreferences ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : ""))
            .alert("마이크", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("확인") { message = nil }
            } message: {
                Text(message ?? "")
            }
    }

    private func update(_ enabled: Bool) {
        let result = session.applyMicrophoneToggle(enabled, permission: permission) { message = $0 }
        isOn = result.isOn
        message = result.message
    }
}

/// While the VM's microphone setting is on but macOS has not allowed access.
struct HvfMicrophoneNoticeView: View {
    @ObservedObject var session: HvfEngineSession
    var permission: any HvfMicrophonePermission = HvfSystemMicrophonePermission()

    var body: some View {
        if session.config.microphoneEnabled, let notice = HvfMicrophoneNotice.text(permission.status) {
            Label(notice, systemImage: "mic.slash").font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("bridgevm.runtime.microphone.notice")
        }
    }
}

extension HvfEngineSession {
    /// Saves the setting and, when it is turned on before macOS has asked, lets macOS ask.
    /// Returns what the toggle shows afterwards plus any message; `notify` gets the
    /// notice if the user then refuses.
    func applyMicrophoneToggle(_ enabled: Bool, permission: any HvfMicrophonePermission,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) },
        notify: @escaping @MainActor (String) -> Void) -> (isOn: Bool, message: String?) {
        var isOn = enabled, message: String?
        if persistsLibraryPreferences {
            message = saveLibraryPreference("마이크 설정", publish: publish) {
                $0.microphoneEnabled = enabled ? true : nil
            }
            isOn = config.libraryContext?.config.microphoneEnabled ?? false
        }
        guard enabled, isOn else { return (isOn, message) }
        switch permission.status {
        case .authorized:
            return (true, message)
        case .denied:
            return (true, message ?? HvfMicrophoneNotice.denied)
        case .undetermined:
            permission.requestAccess { granted in
                if !granted { notify(HvfMicrophoneNotice.denied) }
            }
            return (true, message)
        }
    }
}
