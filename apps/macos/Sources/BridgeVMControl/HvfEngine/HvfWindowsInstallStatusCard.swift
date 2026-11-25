import SwiftUI

struct HvfWindowsInstallStatusCard: View {
    @ObservedObject var session: HvfWindowsInstallSession

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                if session.isRunning {
                    ProgressView().controlSize(.small)
                }
                Text(session.stage.label).font(.headline)
                    .accessibilityIdentifier("bridgevm.windows.install.stage")
                Spacer(minLength: 4)
                if let startedAt = session.startedAt, session.isRunning {
                    HStack(spacing: 4) {
                        Image(systemName: "clock").accessibilityHidden(true)
                        Text(startedAt, style: .timer).monospacedDigit()
                    }
                    .font(.caption).foregroundStyle(.secondary)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("경과 시간")
                    .accessibilityValue(Text(startedAt, style: .timer))
                }
            }
            if let current = session.stage.installationStep {
                HStack(alignment: .top, spacing: 6) {
                    ForEach(HvfWindowsInstallStage.steps.indices, id: \.self) { index in
                        VStack(alignment: .leading, spacing: 5) {
                            Capsule().fill(index <= current ? Color.accentColor : Color.secondary.opacity(0.25))
                                .frame(height: 4)
                            Text(HvfWindowsInstallStage.steps[index])
                                .font(.caption).fontWeight(index == current ? .semibold : .regular)
                                .foregroundStyle(index == current ? .primary : .secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("전체 \(HvfWindowsInstallStage.steps.count)단계 중 \(current + 1)단계")
            }
            if case let .failed(message) = session.stage {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange).accessibilityHidden(true)
                    Text(message).textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("bridgevm.windows.install.failure")
                }
                .font(.callout)
            }
            HvfWindowsInstallControls(session: session)
                .controlSize(.large)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .modifier(LibraryCardSurface())
    }
}
