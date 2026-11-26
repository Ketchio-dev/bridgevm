import SwiftUI

struct RetainedGenericControlView: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("This VM’s registration changed or was removed. Its previous session is shown here.")
                    .foregroundColor(.secondary)
                Text(LibraryRetainedControlDescriptor.generic(config: config, model: model).statusLabel)
                if model.hasActiveRuntimeWork {
                    Button("Stop") { model.stop() }
                        .disabled(!model.retainedStopAvailable)
                        .accessibilityIdentifier("bridgevm.retained.generic.stop")
                    if model.ownedRuntime == nil {
                        Text("Shut down from the original VM window. BridgeVM no longer controls that process.")
                            .font(.caption).foregroundColor(.secondary)
                    }
                } else {
                    Button("Dismiss", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.generic.dismiss")
                }
            }.padding(20)
        }
    }
}
