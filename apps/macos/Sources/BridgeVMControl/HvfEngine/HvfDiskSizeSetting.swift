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
                    Text("Virtual disk size")
                    Text(currentBytes.map(Self.sizeText) ?? "—")
                        .accessibilityLabel(currentBytes.map { "Current size, \(Self.sizeText($0))" } ?? "Current size unknown")
                    if let range {
                        Text("→").accessibilityHidden(true)
                        TextField("New size", text: $targetText)
                            .labelsHidden().frame(width: 52)
                            .accessibilityLabel("New virtual disk size in GiB")
                        Stepper("New virtual disk size", value: Binding(
                            get: { validSize ?? range.lowerBound },
                            set: { targetText = $0.formatted(.number.locale(locale)) }
                        ), in: range).labelsHidden()
                            .fixedSize()
                            .accessibilityValue("\((validSize ?? range.lowerBound).formatted(.number.locale(locale))) GiB")
                        Text("GiB (up to \(range.upperBound) GiB)").font(.caption).foregroundColor(.secondary)
                        Button("Expand disk") { confirmationSize = validSize }
                            .disabled(validSize == nil)
                            .accessibilityIdentifier("bridgevm.runtime.disk-grow")
                    }
                    if busy { ProgressView().controlSize(.small).accessibilityLabel("Expanding the disk") }
                    Spacer()
                }
                .disabled(!stopped || busy)
                Text(note).font(.caption).foregroundColor(.secondary)
                    .accessibilityIdentifier("bridgevm.runtime.disk-grow.status")
            }
            .help(HvfDiskGrow.sparseHelp)
            .onAppear(perform: refresh)
            .onChange(of: stopped) { _, _ in refresh() }
            .confirmationDialog(confirmationSize.map { "Expand the virtual disk to \($0) GiB?" } ?? "",
                isPresented: Binding(get: { confirmationSize != nil }, set: { if !$0 { confirmationSize = nil } }),
                titleVisibility: .visible, presenting: confirmationSize) { size in
                Button("Expand disk") { grow(to: size) }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text(HvfDiskGrow.sparseHelp)
            }
        }
    }

    /// The session's latest message: a grow's result until the guest's extend report replaces it.
    private var note: String {
        if busy { return "Expanding the disk…" }
        if stopped, let range, validSize == nil {
            return "Enter a whole number from \(range.lowerBound.formatted(.number.locale(locale))) to \(range.upperBound.formatted(.number.locale(locale))) GiB."
        }
        if let notice = session.diskGrowNotice { return notice }
        if !stopped { return session.settingsWait(HvfDiskGrow.stopFirst) }
        if currentBytes != nil, range == nil {
            return "To expand the disk, free up more than 10 GiB on the drive that holds this VM."
        }
        return HvfDiskGrow.sparseHelp
    }

    /// Reads the selected disk's size and the volume's free space while stopped.
    private func refresh() {
        guard stopped, !busy, let plan else { return }
        Task {
            let measured = await Task.detached { () -> (UInt64, UInt64)? in
                guard let current = try? HvfDiskGrow.currentBytes(plan) else { return nil }
                let available = HvfImportSpace.available(plan.disk.deletingLastPathComponent())
                    .map { UInt64(max(0, $0)) } ?? 0
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
