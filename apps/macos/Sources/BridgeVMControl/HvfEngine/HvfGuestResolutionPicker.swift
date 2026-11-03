import SwiftUI

/// The Windows display resolution the user chose; nil leaves it to the VMM.
struct HvfGuestResolutionPicker: View {
    static let resolutions = ["1024x768", "1280x800", "1440x900", "1680x1050",
                              "1920x1080", "1920x1200", "2560x1440", "2560x1600"]
    /// Every mode the VMM accepts (`RESOLUTIONS` in crates/bridgevm-hvf/src/uefi_resolution.rs).
    static let firmwareModes = ["640x480", "800x600", "1024x768", "1280x720", "1280x800", "1280x1024",
                                "1366x768", "1440x900", "1600x900", "1600x1200", "1680x1050",
                                "1920x1080", "1920x1200", "2560x1440", "2560x1600"]

    /// The mode the VMM applies for a saved value, read as its `parse_resolution` reads it;
    /// nil (Automatic) for a value it ignores.
    static func firmwareMode(_ value: String?) -> String? {
        let parts = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            .split(separator: "x", maxSplits: 1, omittingEmptySubsequences: false)
        guard parts.count == 2, let width = UInt32(parts[0]), let height = UInt32(parts[1]) else { return nil }
        return firmwareModes.first { $0 == "\(width)x\(height)" }
    }

    struct Entry: Hashable {
        let title: String
        let mode: String?  // nil is Automatic
    }

    /// The menu for a saved value and the mode it shows selected: Automatic, the common modes, and a
    /// mode from a hand-edited vm.json as its own entry. A value the VMM ignores shows Automatic.
    static func menu(for value: String?) -> (entries: [Entry], selected: String?) {
        let selected = firmwareMode(value)
        var entries = [Entry(title: "Automatic (1280x800)", mode: nil)] + resolutions.map { Entry(title: $0, mode: $0) }
        if let selected, !resolutions.contains(selected) {
            entries.append(Entry(title: "\(selected) (custom)", mode: selected))
        }
        return (entries, selected)
    }

    @ObservedObject var session: HvfEngineSession
    @Binding var selection: String?
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        let menu = Self.menu(for: selection)
        Picker("Display resolution", selection: Binding(get: { menu.selected }, set: update)) {
            ForEach(menu.entries, id: \.self) { Text($0.title).tag($0.mode) }
        }
        .fixedSize()
        .accessibilityIdentifier("bridgevm.runtime.guest-resolution")
        .disabled(isDisabled)
        .help(Self.helpText(followsWindow: session.config.virtioGpu2d || session.config.virtioGpu3d,
                            persists: session.persistsLibraryPreferences))
        .alert("화면 해상도 설정", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("확인") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    /// With a virtio display (the signed display driver, or 3D) Windows follows the window after it
    /// starts, so this setting only covers boot and the time before the driver loads.
    static func helpText(followsWindow: Bool, persists: Bool) -> String {
        (followsWindow
            ? "부팅 화면과 디스플레이 드라이버가 로드되기 전의 Windows 해상도입니다. 이 VM은 시작 후 Windows 화면이 창 크기를 따라갑니다."
            : "Windows 화면 해상도입니다. 다음 시작부터 적용되며, Windows가 이전 해상도를 기억하고 있으면 BridgeVM이 시작 후 자동으로 맞춥니다. 창 크기에 맞추려면 VM 화면에서 디스플레이 드라이버를 설치하세요.")
            + (persists ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : "")
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
