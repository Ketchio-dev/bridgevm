import SwiftUI

/// The VM's disk size: it only grows, and only while the VM is fully stopped (HvfDiskGrow).
struct HvfDiskSizeSetting: View {
    @ObservedObject var session: HvfEngineSession
    let config: HvfEngineConfig
    @State private var currentBytes: UInt64?
    @State private var range: ClosedRange<Int>?
    @Environment(\.locale) private var locale
    @State private var targetText = ""
    @State private var busy = false
    @State private var confirmationSize: Int?

    private var plan: HvfDiskGrow.Plan? { HvfDiskGrow.plan(config: config, repoRoot: session.repoRoot) }
    private var stopped: Bool { session.libraryPreferencesEditable && !session.hasActiveRuntimeWork }

    /// Sizes are binary and labelled GiB, as in the create sheet and the dashboard.
    static func sizeText(_ bytes: UInt64) -> String { HvfDiskGrow.gibText(bytes) }

    /// Accept a whole localized GiB amount. Foundation also parses fractions and exponents as integers.
    static func targetSize(_ text: String, in range: ClosedRange<Int>, locale: Locale) -> Int? {
        let input = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let digits = CharacterSet.decimalDigits.union(CharacterSet(charactersIn: locale.groupingSeparator ?? ""))
        guard !input.isEmpty, input.unicodeScalars.allSatisfy(digits.contains) else { return nil }
        guard let size = input.wholeMatch(of: IntegerFormatStyle<Int>.number.locale(locale))?.output,
              range.contains(size) else { return nil }
        return size
    }

    private var validSize: Int? {
        range.flatMap { Self.targetSize(targetText, in: $0, locale: locale) }
    }

    var body: some View {
        if session.persistsLibraryPreferences, plan != nil {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 10) {
                    Text("가상 디스크 용량")
                    Text(currentBytes.map(Self.sizeText) ?? "—")
                    if let range {
                        Text("→")
                        TextField("새 용량", text: $targetText)
                            .labelsHidden().frame(width: 52)
                            .accessibilityLabel("새 가상 디스크 용량 (GiB)")
                        Stepper("가상 디스크 용량", value: Binding(
                            get: { validSize ?? range.lowerBound },
                            set: { targetText = $0.formatted(.number.locale(locale)) }
                        ), in: range).labelsHidden()
                            .fixedSize()
                        Text("GiB (최대 \(range.upperBound) GiB)").font(.caption).foregroundColor(.secondary)
                        Button("디스크 늘리기") { confirmationSize = validSize }
                            .disabled(validSize == nil)
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
            .confirmationDialog(confirmationSize.map { "디스크를 \($0) GiB로 늘리시겠습니까?" } ?? "",
                isPresented: Binding(get: { confirmationSize != nil }, set: { if !$0 { confirmationSize = nil } }),
                titleVisibility: .visible, presenting: confirmationSize) { size in
                Button("디스크 늘리기") { grow(to: size) }
                Button("취소", role: .cancel) {}
            } message: { _ in
                Text(HvfDiskGrow.sparseHelp)
            }
        }
    }

    /// The session's latest message: a grow's result until the guest's extend report replaces it.
    private var note: String {
        if busy { return "디스크를 늘리는 중…" }
        if stopped, let range, validSize == nil {
            return "새 용량은 \(range.lowerBound.formatted(.number.locale(locale)))~\(range.upperBound.formatted(.number.locale(locale))) GiB 사이의 정수로 입력하세요."
        }
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
            if let range, validSize == nil { targetText = range.lowerBound.formatted(.number.locale(locale)) }
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
