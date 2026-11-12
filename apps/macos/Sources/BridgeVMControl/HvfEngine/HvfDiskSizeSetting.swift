import SwiftUI

/// The VM's disk size: it only grows, and only while the VM is fully stopped (HvfDiskGrow).
struct HvfDiskSizeSetting: View {
    @ObservedObject var session: HvfEngineSession
    let config: HvfEngineConfig
    @State private var currentBytes: UInt64?
    @State private var range: ClosedRange<Int>?
    @State private var targetGiB = 0
    @State private var busy = false
    @State private var confirming = false

    private var plan: HvfDiskGrow.Plan? { HvfDiskGrow.plan(config: config, repoRoot: session.repoRoot) }
    private var stopped: Bool { session.libraryPreferencesEditable && !session.hasActiveRuntimeWork }

    /// Display uses the familiar GB label; all sizing and limits remain binary GiB.
    static func sizeText(_ bytes: UInt64) -> String {
        HvfDiskGrow.gibText(bytes).replacingOccurrences(of: " GiB", with: "GB")
    }

    var body: some View {
        if session.persistsLibraryPreferences, plan != nil {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 10) {
                    Text("디스크 크기")
                    Text(currentBytes.map(Self.sizeText) ?? "—")
                    if let range {
                        Text("→")
                        TextField("", value: $targetGiB, format: .number)
                            .labelsHidden().frame(width: 52)
                        Stepper("디스크 크기", value: $targetGiB, in: range).labelsHidden()
                            .fixedSize()
                        Text("GB (최대 \(range.upperBound)GB)").font(.caption).foregroundColor(.secondary)
                        Button("디스크 늘리기") { confirming = range.contains(targetGiB) }
                            .accessibilityIdentifier("bridgevm.runtime.disk-grow")
                    }
                    if busy { ProgressView().controlSize(.small) }
                    Spacer()
                }
                .disabled(!stopped || busy)
                Text(note).font(.caption).foregroundColor(.secondary)
                    .accessibilityIdentifier("bridgevm.runtime.disk-grow.status")
            }
            .help(HvfDiskGrow.sparseHelp)
            .onAppear(perform: refresh)
            .onChange(of: stopped) { _, _ in refresh() }
            .confirmationDialog("디스크를 \(targetGiB)GB로 늘리시겠습니까?", isPresented: $confirming,
                                titleVisibility: .visible) {
                Button("디스크 늘리기") { grow(to: targetGiB) }
                Button("취소", role: .cancel) {}
            } message: {
                Text("늘린 디스크는 다시 줄일 수 없습니다. " + HvfDiskGrow.sparseHelp)
            }
        }
    }

    /// The session's latest message: a grow's result until the guest's extend report replaces it.
    private var note: String {
        if busy { return "디스크를 늘리는 중…" }
        if let notice = session.diskGrowNotice { return notice }
        if !stopped { return HvfDiskGrow.stopFirst }
        if currentBytes != nil, range == nil {
            return "Mac 볼륨에 여유 공간이 부족해 디스크를 늘릴 수 없습니다. 10 GiB 넘게 여유 공간을 확보하세요."
        }
        return HvfDiskGrow.sparseHelp
    }

    /// Reads the selected disk's size and the volume's free space while stopped.
    private func refresh() {
        guard stopped, !busy, let plan else { return }
        Task {
            let measured = await Task.detached { () -> (UInt64, UInt64)? in
                guard let current = try? HvfDiskGrow.currentBytes(plan) else { return nil }
                let values = try? plan.disk.deletingLastPathComponent()
                    .resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
                let available = values?.volumeAvailableCapacityForImportantUsage.map { UInt64(max(0, $0)) } ?? 0
                return (current, available)
            }.value
            guard let (current, available) = measured else { return }
            currentBytes = current
            range = HvfDiskGrow.sizeRange(currentBytes: current, availableBytes: available)
            if let range, !range.contains(targetGiB) { targetGiB = range.lowerBound }
        }
    }

    private func grow(to gib: Int) {
        guard !busy, let plan, let range, range.contains(gib),
              let task = session.growDisk(plan, toGiB: gib) else { return }
        busy = true
        Task {
            await task.value
            busy = false
            refresh()
        }
    }
}
