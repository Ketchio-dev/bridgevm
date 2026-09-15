import SwiftUI

struct LibrarySearchField: View {
    @Binding var query: String
    @FocusState private var isFocused: Bool

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
            TextField("VM 이름 또는 엔진 검색", text: $query)
                .textFieldStyle(.plain)
                .focused($isFocused)
                .accessibilityIdentifier("bridgevm.library.search")
                .help("가상 머신 검색 (⌘F)")
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
            Button("VM 검색") { isFocused = true }
                .keyboardShortcut("f", modifiers: .command)
                .focusable(false)
                .accessibilityHidden(true)
                .opacity(0)
        }
    }
}
