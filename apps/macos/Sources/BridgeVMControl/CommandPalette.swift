import SwiftUI

struct PaletteCommand: Identifiable {
    let id: String
    let title: String
    let subtitle: String
    let systemImage: String
    let action: () -> Void
}

/// ⌘K command palette — invoke any VM action by name (Pro power surface).
struct CommandPaletteView: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var selectedID: String?
    @State private var focusRequest = 0

    private var commands: [PaletteCommand] {
        var c = library.selectedPaletteCommands { dismiss() }
        c.append(.init(id: "create", title: "새 VM 만들기", subtitle: "Windows 11", systemImage: "plus") { library.showingCreate = true; dismiss() })
        c.append(.init(id: "pro-mode", title: library.proMode ? "Pro 모드 끄기" : "Pro 모드 켜기", subtitle: "전체 VM 테이블", systemImage: "tablecells") { library.proMode.toggle(); dismiss() })
        let named = Dictionary(grouping: library.vms, by: \.name)
        for vm in library.vms {
            let subtitle = named[vm.name]?.count == 1 ? "이 VM 선택" : "이 VM 선택 · \(vm.slug)"
            c.append(.init(id: "navigate.\(vm.slug)", title: "이동: \(vm.name)", subtitle: subtitle, systemImage: "desktopcomputer") {
                library.selectedID = vm.slug; library.proMode = false; dismiss()
            })
        }
        return c
    }

    private var filtered: [PaletteCommand] {
        query.isEmpty ? commands
            : commands.filter { $0.title.localizedCaseInsensitiveContains(query) || $0.subtitle.localizedCaseInsensitiveContains(query) }
    }

    private func choice(in matches: [PaletteCommand]) -> PaletteCommand? {
        if let selectedID { return matches.first { $0.id == selectedID } }
        return !query.isEmpty && matches.count == 1 ? matches.first : nil
    }

    private func setQuery(_ value: String) {
        if value != query { selectedID = nil; query = value }
    }

    private func move(down: Bool) {
        let matches = filtered
        guard !matches.isEmpty else { return }
        let current = matches.firstIndex { $0.id == selectedID }
        let last = matches.count - 1
        let next = down ? min((current ?? -1) + 1, last) : max((current ?? matches.count) - 1, 0)
        selectedID = matches[next].id
    }

    var body: some View {
        let matches = filtered
        let returnCommand = choice(in: matches)
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
                PaletteSearchField(text: Binding(get: { query }, set: setQuery), focusRequest: focusRequest,
                                   move: move, submit: { choice(in: filtered)?.action() })
                if !query.isEmpty {
                    Button {
                        setQuery("")
                        focusRequest += 1
                    } label: {
                        Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("검색어 지우기")
                }
            }
            .padding(14)
            Divider()
            PaletteResults(matches: matches, selectedID: selectedID, returnID: returnCommand?.id)
        }
        .frame(width: 480, height: 400)
    }
}

private struct PaletteResults: View {
    let matches: [PaletteCommand]
    let selectedID: String?
    let returnID: String?

    var body: some View {
        ScrollViewReader { reader in
            List(matches) { cmd in
                Button(action: cmd.action) {
                    HStack(spacing: 10) {
                        Image(systemName: cmd.systemImage).frame(width: 22).foregroundStyle(.tint)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(cmd.title)
                            Text(cmd.subtitle).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if cmd.id == returnID {
                            Image(systemName: "return").foregroundStyle(.secondary)
                                .accessibilityLabel("Return 키로 실행")
                        }
                    }
                    .padding(.vertical, 3)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(cmd.id == selectedID ? .isSelected : [])
                .listRowBackground(cmd.id == selectedID ? Color.accentColor.opacity(0.16) : Color.clear)
            }
            .onChange(of: selectedID) { _, id in
                if let id { reader.scrollTo(id) }
            }
        }
        .overlay {
            if matches.isEmpty {
                ContentUnavailableView("일치하는 명령이 없습니다", systemImage: "magnifyingglass",
                    description: Text("다른 명령이나 VM 이름으로 검색하세요."))
            }
        }
    }
}
