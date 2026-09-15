import SwiftUI

struct HvfRuntimeHeader: View {
    let title: String
    let state: String
    let stateColor: Color
    let ramMiB: Int
    let cpus: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(alignment: .top, spacing: 20) {
                LibraryVMEmblem(engine: .hvfEngine)
                VStack(alignment: .leading, spacing: 8) {
                    Text(title).font(.system(size: 28, weight: .bold)).textSelection(.enabled)
                    Text("HVF Engine (Experimental)").font(.callout.weight(.medium)).foregroundStyle(.secondary)
                    Label {
                        Text(state).font(.callout.weight(.medium)).textSelection(.enabled)
                    } icon: {
                        Circle().fill(stateColor).frame(width: 8, height: 8)
                    }
                    .padding(.horizontal, 10).padding(.vertical, 6)
                    .background(stateColor.opacity(0.1), in: Capsule())
                    .accessibilityElement(children: .combine)
                }
                Spacer(minLength: 0)
            }
            Divider()
            HStack(spacing: 32) {
                LibraryMetadataLabel(title: "CPU 구성", value: "\(cpus) vCPU", symbol: "cpu")
                LibraryMetadataLabel(title: "메모리 구성", value: memory, symbol: "memorychip")
                Spacer(minLength: 0)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(LibraryCardSurface())
    }

    private var memory: String {
        ramMiB.isMultiple(of: 1024) ? "\(ramMiB / 1024) GiB" : "\(ramMiB) MiB"
    }
}
