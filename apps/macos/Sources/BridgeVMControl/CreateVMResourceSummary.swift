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
