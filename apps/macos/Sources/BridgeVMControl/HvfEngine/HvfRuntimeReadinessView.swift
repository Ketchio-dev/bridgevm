import SwiftUI

struct HvfRuntimeReadinessView: View {
    @ObservedObject var model: HvfRuntimeReadinessModel
    let configuration: HvfEngineConfig
    let repoRoot: URL

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let report = model.report(configuration: configuration, repoRoot: repoRoot) {
                HvfWindowsReadinessCard(report: report)
            } else {
                GroupBox {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text("Checking startup readiness…").foregroundStyle(.secondary)
                        Spacer()
                    }.padding(6)
                } label: {
                    Label("Windows startup readiness", systemImage: "checklist")
                }
            }
            Button {
                model.request(configuration: configuration, repoRoot: repoRoot, force: true)
            } label: {
                Label("Refresh readiness", systemImage: "arrow.clockwise")
            }
            .disabled(model.isChecking)
            .accessibilityIdentifier("bridgevm.windows.runtime.readiness.refresh")
        }
    }
}
