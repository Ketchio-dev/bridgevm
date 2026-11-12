import SwiftUI

/// Uses the runtime controls and their existing save path, not a second preferences model.
struct HvfVMSettingsSheet: View {
    @ObservedObject var session: HvfEngineSession
    @Environment(\.dismiss) private var dismiss
    var captureHeight: CGFloat = 720
    @State private var shareEnabled: Bool
    @State private var shareHostDir: String
    @State private var shareGuestDir: String
    @State private var portForwards: String

    init(session: HvfEngineSession) {
        self.session = session
        let config = session.config
        _shareEnabled = State(initialValue: config.shareHostDir != nil && config.shareGuestDir != nil)
        _shareHostDir = State(initialValue: config.shareHostDir ?? "")
        _shareGuestDir = State(initialValue: config.shareGuestDir ?? "C:\\bridgevm-share")
        _portForwards = State(initialValue: HvfPortForwards.text(config.portForwards))
    }

    private func binding<Value>(_ keyPath: WritableKeyPath<HvfEngineConfig, Value>) -> Binding<Value> {
        Binding(get: { session.config[keyPath: keyPath] }, set: { session.config[keyPath: keyPath] = $0 })
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("설정").font(.title2.bold())
                Spacer()
                Button("완료") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            .padding(20)
            Form {
                if let notice = runningNoticeText {
                    Text(notice).foregroundStyle(.secondary)
                }
                Section("하드웨어") {
                    HvfResourceSetting(session: session, ramMiB: binding(\.ramMiB), cpus: binding(\.smpCpus))
                    HvfDiskSizeSetting(session: session, config: session.config)
                }
                Section("디스플레이") {
                    if session.config.offersRetinaResolution && !session.config.virtioGpu3d {
                        HvfRetinaResolutionToggle(session: session, setting: binding(\.retinaResolution))
                        help("Retina 화면의 픽셀 수로 그립니다. CPU 사용량이 늘어날 수 있습니다.")
                    }
                    HvfGuestResolutionPicker(session: session, selection: binding(\.guestResolution))
                    help("디스플레이 드라이버를 쓰면 시작 후 화면은 창 크기를 따릅니다.")
                    HvfDisplayDriverInstallButton(session: session) { _ in HvfDisplayDriverFlag.record(session) }
                    HvfHardwareCursorToggle(session: session)
                }
                Section("공유") {
                    HvfSharedFolderSettings(session: session, enabled: $shareEnabled,
                        hostDir: $shareHostDir, guestDir: $shareGuestDir)
                    HvfClipboardSyncToggle(session: session, isOn: binding(\.clipboardSync))
                    HvfGuestPrintToggle(session: session)
                    HvfGuestLinksToggle(session: session)
                }
                Section("네트워크") {
                    HvfNetworkToggle(session: session, isOn: binding(\.virtioNet))
                    if session.config.virtioNet {
                        HvfPortForwardSetting(session: session, text: $portForwards)
                    }
                }
                Section("입력") {
                    HvfCommandKeyPicker(session: session, selection: binding(\.commandKeyMode))
                }
                Section("마이크") {
                    HvfMicrophoneToggle(session: session, isOn: binding(\.microphoneEnabled))
                }
                Section("디스크") {
                    HvfDurableDiskWritesToggle(session: session, isOn: binding(\.durableDiskWrites))
                    help("켜면 Windows의 쓰기 확인 전에 디스크에 안전하게 저장하지만, 쓰기가 느려집니다.")
                }
                Section("스냅샷") {
                    HvfWindowsSnapshotCard(config: session.config, session: session)
                }
            }
            .formStyle(.grouped)
        }
        .frame(width: 680, height: captureHeight)
        .accessibilityIdentifier("bridgevm.runtime.settings")
    }

    static let runningNotice = "일부 설정은 Windows를 종료한 뒤 바꿀 수 있습니다."
    var runningNoticeText: String? { session.hasActiveRuntimeWork ? Self.runningNotice : nil }

    private func help(_ text: String) -> some View {
        Text(text).font(.caption).foregroundStyle(.secondary)
    }
}

struct HvfVMSettingsActionKey: FocusedValueKey {
    typealias Value = () -> Void
}
extension FocusedValues {
    var hvfVMSettings: (() -> Void)? {
        get { self[HvfVMSettingsActionKey.self] }
        set { self[HvfVMSettingsActionKey.self] = newValue }
    }
}
