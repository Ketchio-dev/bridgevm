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
                        Text("부팅 준비 상태를 확인하고 있습니다…").foregroundStyle(.secondary)
                        Spacer()
                    }.padding(6)
                } label: {
                    Label("Windows HVF Readiness", systemImage: "checklist")
                }
            }
            Button {
                model.request(configuration: configuration, repoRoot: repoRoot, force: true)
            } label: {
                Label("준비 상태 새로고침", systemImage: "arrow.clockwise")
            }
            .disabled(model.isChecking)
            .accessibilityIdentifier("bridgevm.windows.runtime.readiness.refresh")
        }
    }
}
