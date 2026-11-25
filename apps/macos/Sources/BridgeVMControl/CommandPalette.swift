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
    @FocusState private var searchFocused: Bool

    private var commands: [PaletteCommand] {
        var c = library.selectedPaletteCommands { dismiss() }
        c.append(.init(id: "create", title: "새 VM 만들기", subtitle: "Windows 11", systemImage: "plus") { library.showingCreate = true; dismiss() })
        c.append(.init(id: "pro-mode", title: library.proMode ? "Pro 모드 끄기" : "Pro 모드 켜기", subtitle: "전체 VM 테이블", systemImage: "tablecells") { library.proMode.toggle(); dismiss() })
        for vm in library.vms {
            c.append(.init(id: "navigate.\(vm.slug)", title: "이동: \(vm.name)", subtitle: "이 VM 선택", systemImage: "desktopcomputer") {
                library.selectedID = vm.slug; library.proMode = false; dismiss()
            })
        }
        return c
    }

    private var filtered: [PaletteCommand] {
        query.isEmpty ? commands
            : commands.filter { $0.title.localizedCaseInsensitiveContains(query) || $0.subtitle.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        let matches = filtered
        let returnCommand = !query.isEmpty && matches.count == 1 ? matches.first : nil
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
                TextField("명령 또는 VM 검색", text: $query)
                    .textFieldStyle(.plain).font(.title3)
                    .focused($searchFocused)
                    .onSubmit { returnCommand?.action() }
                if !query.isEmpty {
                    Button {
                        query = ""
                        searchFocused = true
                    } label: {
                        Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("검색어 지우기")
                }
            }
            .padding(14)
            Divider()
            List(matches) { cmd in
                Button(action: cmd.action) {
                    HStack(spacing: 10) {
                        Image(systemName: cmd.systemImage).frame(width: 22).foregroundStyle(.tint)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(cmd.title)
                            Text(cmd.subtitle).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if cmd.id == returnCommand?.id {
                            Image(systemName: "return").foregroundStyle(.secondary)
                                .accessibilityLabel("Return 키로 실행")
                        }
                    }
                    .padding(.vertical, 3)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .overlay {
                if matches.isEmpty {
                    ContentUnavailableView("일치하는 명령이 없습니다", systemImage: "magnifyingglass",
                        description: Text("다른 명령이나 VM 이름으로 검색하세요."))
                }
            }
        }
        .frame(width: 480, height: 400)
        .onAppear { searchFocused = true }
    }
}
