import AVFoundation
import SwiftUI

/// macOS's answer to this app's microphone access, as the setting needs it.
enum HvfMicrophoneStatus: Equatable {
    case authorized, undetermined, denied
}

/// macOS's microphone permission for this app, behind a protocol so that no test
/// reaches it: only the user turning the setting on, or starting a VM with it on, may make macOS ask.
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
    static let denied = "Windows has no microphone because macOS denied access. Allow BridgeVM Control in System Settings > Privacy & Security > Microphone, then restart the VM."
    static let undetermined = "Windows has no microphone yet. macOS asks for access the next time you start this VM from BridgeVM."
    static let restart = "macOS now allows the microphone. Restart the VM so Windows gets it."

    /// `runHasMicrophone`: what the current run started with (nil while no run of this app's is going). Windows'
    /// microphone is fixed at the start, so the notice follows that, not a permission changed since.
    static func text(_ status: HvfMicrophoneStatus, runHasMicrophone: Bool?) -> String? {
        switch runHasMicrophone {
        case true?: return nil
        case false? where status == .authorized: return restart
        default: return text(status)
        }
    }

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
        Toggle("Use microphone", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.microphone")
            .disabled(isDisabled)
            .help("Windows records from the Mac’s default input device when this is on. It is off by default. Changes take effect on the next VM start."
                + (session.persistsLibraryPreferences ? " Changes made while the VM is stopped are saved for future starts." : ""))
            .alert("Microphone", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("OK") { message = nil }
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
        if session.config.microphoneEnabled,
           let notice = HvfMicrophoneNotice.text(permission.status, runHasMicrophone: session.ownedController?.microphone) {
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
            message = saveLibraryPreference("Microphone", publish: publish) {
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
