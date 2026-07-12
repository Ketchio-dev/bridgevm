import SwiftUI

struct PaletteCommand: Identifiable {
    let id = UUID()
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

    private var commands: [PaletteCommand] {
        var c: [PaletteCommand] = []
        if let m = library.selectedModel {
            if !m.running && !m.lifecycleBusy {
                c.append(.init(title: "시작: \(m.config.name)", subtitle: "VM 시작 / 창 열기", systemImage: "play.fill") { m.start(); dismiss() })
            } else if m.running && !m.lifecycleBusy {
                c.append(.init(title: "정지: \(m.config.name)", subtitle: "VM 정지", systemImage: "stop.fill") { m.stop(); dismiss() })
            }
            c.append(.init(title: "새로고침: \(m.config.name)", subtitle: "상태 갱신", systemImage: "arrow.clockwise") { m.refresh(); dismiss() })
        }
        c.append(.init(title: "새 VM 만들기", subtitle: "Ubuntu / Linux ISO / Windows 11", systemImage: "plus") { library.showingCreate = true; dismiss() })
        c.append(.init(title: library.proMode ? "Pro 모드 끄기" : "Pro 모드 켜기", subtitle: "전체 VM 테이블", systemImage: "tablecells") { library.proMode.toggle(); dismiss() })
        for vm in library.vms {
            c.append(.init(title: "이동: \(vm.name)", subtitle: "이 VM 선택", systemImage: "desktopcomputer") {
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
        VStack(spacing: 0) {
            TextField("명령 검색…", text: $query)
                .textFieldStyle(.plain).font(.title3).padding(14)
            Divider()
            List(filtered) { cmd in
                Button(action: cmd.action) {
                    HStack(spacing: 10) {
                        Image(systemName: cmd.systemImage).frame(width: 22).foregroundColor(.accentColor)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(cmd.title)
                            Text(cmd.subtitle).font(.caption).foregroundColor(.secondary)
                        }
                        Spacer()
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .frame(width: 480, height: 400)
    }
}
