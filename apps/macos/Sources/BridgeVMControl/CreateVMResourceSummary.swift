import SwiftUI

struct CreateVMResourceSummary: View {
    let cpuCount: Int
    let ramMiB: Int
    let diskGiB: Int?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("선택한 사양").font(.caption.weight(.medium))
            Text("\(cpuCount) vCPU · 메모리 \(ramMiB / 1024) GiB"
                 + (diskGiB.map { " · 디스크 \($0) GiB" } ?? ""))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("bridgevm.create.resource-summary")
    }
}

struct CreateVMMemoryOptions {
    static let current = CreateVMMemoryOptions(hostBytes: ProcessInfo.processInfo.physicalMemory)

    let choicesMiB: [Int]
    var recommendedMiB: Int { choicesMiB.contains(6144) ? 6144 : 4096 }

    init(hostBytes: UInt64) {
        // Preserve 4 GiB for macOS on larger hosts and offer 4 GiB on an 8 GiB Mac.
        let ceilingMiB = max(4096, Int(hostBytes / (1024 * 1024)) - 4096)
        choicesMiB = [2048, 4096, 6144, 8192, 12288, 16384, 24576, 32768]
            .filter { $0 <= ceilingMiB }
    }
}
