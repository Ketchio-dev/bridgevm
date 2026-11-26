import SwiftUI

/// Uses the runtime controls and their existing save path, not a second preferences model.
struct HvfVMSettingsSheet: View {
    @ObservedObject var session: HvfEngineSession
    @Environment(\.dismiss) private var dismiss
    var captureHeight: CGFloat = 720
    @StateObject private var draft: HvfVMSettingsDraft

    init(session: HvfEngineSession) {
        self.session = session
        _draft = StateObject(wrappedValue: HvfVMSettingsDraft(session: session))
    }

    private func binding<Value>(_ keyPath: WritableKeyPath<HvfEngineConfig, Value>) -> Binding<Value> {
        Binding(get: { session.config[keyPath: keyPath] }, set: { session.config[keyPath: keyPath] = $0 })
    }

    private enum Destination: String, CaseIterable {
        case general, display, sharing, network, snapshots

        var item: (title: String, icon: String, shortcut: KeyEquivalent) {
            switch self {
            case .general: ("일반", "gearshape", "1")
            case .display: ("디스플레이", "display", "2")
            case .sharing: ("공유", "folder", "3")
            case .network: ("네트워크", "network", "4")
            case .snapshots: ("스냅샷", "clock.arrow.circlepath", "5")
            }
        }
    }

    var body: some View {
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text("설정").font(.title2.bold())
                        if let vmName = session.config.libraryContext?.config.displayName {
                            Text(vmName).foregroundStyle(.secondary).lineLimit(1)
                        }
                        Spacer()
                        Button("완료") { finish(proxy) }
                            .keyboardShortcut(.cancelAction)
                            .accessibilityIdentifier("bridgevm.runtime.settings.done")
                    }
                    HStack(spacing: 8) {
                        ForEach(Destination.allCases, id: \.self) { destination in
                            let item = destination.item
                            Button { proxy.scrollTo(destination, anchor: .top) } label: {
                                Label(item.title, systemImage: item.icon)
                            }
                            .keyboardShortcut(item.shortcut, modifiers: .command)
                            .help("\(item.title) 설정으로 이동 (⌘\(String(item.shortcut.character)))")
                            .accessibilityIdentifier("bridgevm.runtime.settings.\(destination.rawValue)")
                        }
                    }
                    .buttonStyle(.accessoryBar)
                    if let notice = runningNoticeText {
                        Label(notice, systemImage: "info.circle").font(.callout).foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 14)
                Divider()
                if let library = session.library {
                    VMPreparationQuitStatus(library: library)
                }
                settingsForm
            }
            .background(HvfSettingsDismissGuard(shouldDismiss: { validate(proxy) }))
            .focusedSceneValue(\.hvfVMSettings, { finish(proxy) })
        }
        .frame(width: 680, height: captureHeight)
        // Without a container element the sheet's identifier replaces the 완료 button's own.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bridgevm.runtime.settings")
        .onAppear { session.settingsSheetOpen = true }
        .onChange(of: session.config.libraryContext?.config.name) { draft.refreshName() }
        .onDisappear {
            draft.commitShare()
            draft.commitForwards()
            session.settingsSheetOpen = false
        }
        .interactiveDismissDisabled()
    }

    var settingsForm: some View {
        Form {
            HvfStartCloseSettings(session: session, draft: draft)
                .id(Destination.general)
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
            .id(Destination.display)
            Section("공유") {
                HvfSharedFolderSettings(session: session, enabled: $draft.shareEnabled,
                    hostDir: $draft.host, guestDir: $draft.guest, commit: { draft.commitShare() })
                if let message = draft.shareMessage { validationMessage(message) }
                HvfClipboardSyncToggle(session: session, isOn: binding(\.clipboardSync))
                HvfGuestPrintToggle(session: session)
                HvfGuestLinksToggle(session: session)
            }
            .id(Destination.sharing)
            Section("네트워크") {
                HvfNetworkToggle(session: session, isOn: binding(\.virtioNet))
                if session.config.virtioNet {
                    HvfPortForwardSetting(session: session, text: $draft.forwards,
                        commit: { draft.commitForwards() })
                    if let message = draft.forwardsMessage { validationMessage(message) }
                }
            }
            .id(Destination.network)
            Section("스냅샷") {
                HvfWindowsSnapshotCard(config: session.config, session: session)
            }
            .id(Destination.snapshots)
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
        }
        .formStyle(.grouped)
    }

    private func validate(_ proxy: ScrollViewProxy) -> Bool {
        if draft.finish() { return true }
        if draft.nameMessage != nil {
            proxy.scrollTo(Destination.general, anchor: .top)
        } else {
            proxy.scrollTo(draft.shareMessage == nil ? Destination.network : .sharing, anchor: .top)
        }
        return false
    }

    private func finish(_ proxy: ScrollViewProxy) {
        if validate(proxy) { dismiss() }
    }

    static let runningNotice = "일부 설정은 Windows를 종료한 뒤 바꿀 수 있습니다."
    var runningNoticeText: String? { session.hasActiveRuntimeWork ? Self.runningNotice : nil }

    private func validationMessage(_ text: String) -> some View {
        Label {
            Text(text)
        } icon: {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        }
        .font(.callout)
    }

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
