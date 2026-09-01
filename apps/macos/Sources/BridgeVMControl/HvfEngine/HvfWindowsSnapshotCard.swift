import SwiftUI

struct HvfWindowsSnapshotCard: View {
    let config: HvfEngineConfig
    let repoRoot: URL
    let vmStopped: Bool
    @State private var busy = false
    @State private var status = "VM을 종료한 뒤 디스크와 UEFI vars를 한 쌍으로 저장할 수 있습니다."

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Button("스냅샷 생성") { perform(.create) }
                        .accessibilityIdentifier("bridgevm.runtime.snapshot.create")
                    Button("스냅샷 복원") { perform(.restore) }
                        .accessibilityIdentifier("bridgevm.runtime.snapshot.restore")
                    if busy { ProgressView().controlSize(.small) }
                    Spacer()
                }
                .disabled(!vmStopped || busy)
                Text(status)
                    .font(.caption)
                    .foregroundColor(status.hasPrefix("스냅샷 실패:") ? .red : .secondary)
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.status")
            }
            .padding(6)
        } label: {
            Label("Powered-off Snapshot", systemImage: "clock.arrow.circlepath")
        }
    }

    private func perform(_ operation: HvfWindowsSnapshotCommand.Operation) {
        guard vmStopped, !busy else { return }
        busy = true
        status = operation == .create ? "스냅샷 생성 중…" : "스냅샷 복원 중…"
        do {
            let plan = try HvfWindowsSnapshotCommand.plan(
                config: config, repoRoot: repoRoot, operation: operation)
            Task {
                do {
                    _ = try await HvfWindowsSnapshotCommand.run(operation, plan: plan)
                    status = operation == .create ? "스냅샷 생성 완료" : "스냅샷 복원 완료"
                } catch {
                    status = "스냅샷 실패: \(error.localizedDescription)"
                }
                busy = false
            }
        } catch {
            status = "스냅샷 실패: \(error.localizedDescription)"
            busy = false
        }
    }
}
