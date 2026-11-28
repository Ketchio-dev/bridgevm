import SwiftUI

struct LibraryBrand: View {
    var body: some View {
        HStack(spacing: 12) {
            BridgeVMMark()
                .fill(LibraryAppearance.accent)
                .frame(width: 42, height: 42)
                .accessibilityHidden(true)
            Text("BridgeVM").font(.system(size: 23, weight: .bold))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 22)
    }
}

struct LibraryHostSummary: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Mac hardware", systemImage: "desktopcomputer")
                .font(.callout.weight(.semibold))
            HStack(spacing: 18) {
                LibraryMetadataLabel(title: "Memory", value: "\(Int(library.hostMemGiB.rounded())) GB", symbol: "memorychip")
                LibraryMetadataLabel(title: "CPUs", value: "\(library.hostCPU)", symbol: "cpu")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .modifier(LibraryCardSurface())
        .help("Total memory and CPUs available on this Mac.")
    }
}
