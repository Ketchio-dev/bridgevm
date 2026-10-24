import SwiftUI

struct LibrarySearchField: View {
    @Binding var query: String
    @State private var focusRequests = 0

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
            LibrarySearchTextField(query: $query, focusRequests: focusRequests)
            if !query.isEmpty {
                Button { query = "" } label: {
                    Image(systemName: "xmark.circle.fill")
                        .frame(width: 32, height: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain).foregroundStyle(.secondary)
                .accessibilityLabel("검색 지우기")
            }
        }
        .frame(minHeight: 32)
        .padding(12)
        .background(LibraryAppearance.inset, in: RoundedRectangle(cornerRadius: 10))
        .background {
            Button("VM 검색") { focusRequests += 1 }
                .keyboardShortcut("f", modifiers: .command)
                .focusable(false)
                .accessibilityHidden(true)
                .opacity(0)
        }
    }
}
