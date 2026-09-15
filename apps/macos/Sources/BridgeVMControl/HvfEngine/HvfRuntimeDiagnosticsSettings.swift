import SwiftUI

struct HvfRuntimeDiagnosticsSettings<PathRow: View>: View {
    @Binding var evidenceDir: String
    @Binding var ctlFilePath: String
    @Binding var watchdogEnabled: Bool
    @Binding var watchdogMs: Int
    @Binding var nvmeBufferedIO: Bool
    @Binding var ctlInput: String
    let sendCtl: () -> Void
    let pathRow: (String, Binding<String>, Bool) -> PathRow
    @State private var expanded = false

    var body: some View {
        GroupBox {
            DisclosureGroup("고급 진단", isExpanded: $expanded) {
                VStack(alignment: .leading, spacing: 12) {
                    pathRow("Evidence dir", $evidenceDir, true)
                    pathRow("CTL file", $ctlFilePath, false)
                    HStack {
                        Text("Watchdog").frame(width: 92, alignment: .leading)
                        Toggle("Enabled", isOn: $watchdogEnabled)
                            .toggleStyle(.checkbox)
                        Stepper("\(watchdogMs) ms", value: $watchdogMs, in: 60_000...86_400_000, step: 30_000)
                            .font(.body.monospaced())
                            .disabled(!watchdogEnabled)
                        Spacer()
                    }
                    Toggle("Buffered NVMe (diagnostic)", isOn: $nvmeBufferedIO)
                    HStack(spacing: 10) {
                        Button(action: sendCtl) { Label("Send", systemImage: "paperplane.fill") }
                            .disabled(ctlInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty).accessibilityIdentifier("bridgevm.runtime.ctl.send")
                        TextField("CLIPGET, CLIPSET ..., or guest shell command", text: $ctlInput, onCommit: sendCtl)
                            .textFieldStyle(.roundedBorder).font(.body.monospaced()).accessibilityIdentifier("bridgevm.runtime.ctl.input")
                    }
                }
                .padding(.top, 12)
            }
            .padding(6)
        }
    }
}
