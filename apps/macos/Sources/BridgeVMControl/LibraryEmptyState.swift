import SwiftUI

struct LibraryEmptyState: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "desktopcomputer").font(.system(size: 48)).foregroundColor(.secondary)
            Text("VM을 선택하거나 새로 만드세요").foregroundColor(.secondary)
            Button { library.showingCreate = true } label: { Label("새 VM", systemImage: "plus") }
                .accessibilityIdentifier("bridgevm.library.empty.create")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
