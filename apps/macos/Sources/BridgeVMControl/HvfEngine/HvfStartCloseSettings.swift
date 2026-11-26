import SwiftUI

struct HvfStartCloseSettings: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var draft: HvfVMSettingsDraft
    @ObservedObject var power = ControlAppPower.shared
    @AppStorage(ControlDisplayClose.preferenceKey) private var closeChoice = ""
    @State private var message: String?
    var body: some View {
        Section("General") {
            if session.persistsLibraryPreferences {
                TextField("VM name", text: $draft.name)
                    .onSubmit { draft.commitName() }
                    .disabled(!session.libraryPreferencesEditable)
                    .accessibilityIdentifier("bridgevm.runtime.settings.vm-name")
                if let message = draft.nameMessage {
                    Label(message, systemImage: "exclamationmark.triangle").font(.callout)
                }
            }
            Toggle("Keep Mac awake while Windows is running", isOn: Binding(
                get: { power.enabled }, set: { power.setEnabled($0) }))
            Text("The Mac display can still turn off.")
                .font(.caption).foregroundStyle(.secondary)
            Picker("When the VM window closes", selection: $closeChoice) {
                Text("Ask each time").tag("")
                ForEach([ControlDisplayCloseChoice.background, .pause, .shutdown], id: \.rawValue) {
                    Text($0.title).tag($0.rawValue)
                }
            }
            if session.persistsLibraryPreferences {
                Toggle("Start this VM when BridgeVM opens", isOn: Binding(
                    get: { session.config.libraryContext?.config.startWithBridgeVM == true },
                    set: { value in
                        message = session.saveLibraryPreference("Automatic startup",
                            publish: { VMRegistrationWriter.commit($0, to: $1) }) { $0.startWithBridgeVM = value }
                    }))
                    .disabled(!session.libraryPreferencesEditable)
                Text("Hold Option while opening BridgeVM to skip automatic startup.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let message { Text(message).foregroundStyle(.secondary) }
        }
    }
}
