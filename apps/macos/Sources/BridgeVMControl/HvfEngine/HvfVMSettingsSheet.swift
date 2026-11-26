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
            case .general: ("General", "gearshape", "1")
            case .display: ("Display", "display", "2")
            case .sharing: ("Sharing", "folder", "3")
            case .network: ("Network", "network", "4")
            case .snapshots: ("Snapshots", "clock.arrow.circlepath", "5")
            }
        }
    }

    var body: some View {
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text("Settings").font(.title2.bold())
                        if let vmName = session.config.libraryContext?.config.displayName {
                            Text(vmName).foregroundStyle(.secondary).lineLimit(1)
                        }
                        Spacer()
                        Button("Done") { finish(proxy) }
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
                            .help("\(item.title) settings (⌘\(String(item.shortcut.character)))")
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
        // Without a container element the sheet's identifier replaces the Done button's own.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bridgevm.runtime.settings")
        .onAppear(perform: draft.didAppear)
        .onChange(of: session.config.libraryContext?.config.name) { draft.refreshName() }
        .onDisappear(perform: draft.didDisappear)
        .interactiveDismissDisabled()
    }

    var settingsForm: some View {
        Form {
            HvfStartCloseSettings(session: session, draft: draft)
                .id(Destination.general)
            Section("Hardware") {
                HvfResourceSetting(session: session, ramMiB: binding(\.ramMiB), cpus: binding(\.smpCpus))
                HvfDiskSizeSetting(session: session, config: session.config)
            }
            Section("Display") {
                if session.config.offersRetinaResolution && !session.config.virtioGpu3d {
                    HvfRetinaResolutionToggle(session: session, setting: binding(\.retinaResolution))
                    help("Render at the Retina display’s native resolution. This can increase CPU usage.")
                }
                HvfGuestResolutionPicker(session: session, selection: binding(\.guestResolution))
                help("With the display driver installed, Windows adjusts its resolution to fit the window after startup.")
                HvfDisplayDriverInstallButton(session: session) { _ in HvfDisplayDriverFlag.record(session) }
                HvfHardwareCursorToggle(session: session)
            }
            .id(Destination.display)
            Section("Sharing") {
                HvfSharedFolderSettings(session: session, enabled: $draft.shareEnabled,
                    hostDir: $draft.host, guestDir: $draft.guest, commit: { draft.commitShare() })
                if let message = draft.shareMessage { validationMessage(message) }
                HvfClipboardSyncToggle(session: session, isOn: binding(\.clipboardSync))
                HvfGuestPrintToggle(session: session)
                HvfGuestLinksToggle(session: session)
            }
            .id(Destination.sharing)
            Section("Network") {
                HvfNetworkToggle(session: session, isOn: binding(\.virtioNet))
                if session.config.virtioNet {
                    HvfPortForwardSetting(session: session, text: $draft.forwards,
                        commit: { draft.commitForwards() })
                    if let message = draft.forwardsMessage { validationMessage(message) }
                }
            }
            .id(Destination.network)
            Section("Input") {
                HvfCommandKeyPicker(session: session, selection: binding(\.commandKeyMode))
            }
            Section("Microphone") {
                HvfMicrophoneToggle(session: session, isOn: binding(\.microphoneEnabled))
            }
            Section("Disk") {
                HvfDurableDiskWritesToggle(session: session, isOn: binding(\.durableDiskWrites))
            }
            Section("Snapshots") {
                HvfWindowsSnapshotCard(config: session.config, session: session)
            }
            .id(Destination.snapshots)
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

    static let runningNotice = "Shut down Windows to change settings that are currently unavailable."
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
