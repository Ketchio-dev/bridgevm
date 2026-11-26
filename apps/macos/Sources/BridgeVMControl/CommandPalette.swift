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
    let createAction: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var selectedID: String?
    @State private var focusRequest = 0

    private var commands: [PaletteCommand] {
        var c = library.selectedPaletteCommands { dismiss() }
        c.append(.init(id: "create", title: "New Windows VM", subtitle: "Create a Windows 11 virtual machine", systemImage: "plus", action: createAction))
        c.append(.init(id: "pro-mode", title: library.proMode ? "Turn off Pro mode" : "Turn on Pro mode", subtitle: library.proMode ? "Return to the standard library" : "Show all virtual machines in a table", systemImage: "tablecells") { library.proMode.toggle(); dismiss() })
        c.append(.init(id: "reload-library", title: "Reload Library", subtitle: "Refresh saved VMs from disk",
                       systemImage: "arrow.clockwise") {
            library.reload()
            dismiss()
        })
        let named = Dictionary(grouping: library.vms, by: \.name)
        for vm in library.vms {
            let subtitle = named[vm.name]?.count == 1 ? "Select this VM" : "Select this VM · \(vm.slug)"
            c.append(.init(id: "navigate.\(vm.slug)", title: "Go to \(vm.name)", subtitle: subtitle, systemImage: "desktopcomputer") {
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
                    .accessibilityLabel("Clear search")
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
                                .accessibilityLabel("Press Return to run")
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
                if let id { reader.scrollTo(id, anchor: .center) }
            }
        }
        .overlay {
            if matches.isEmpty {
                ContentUnavailableView("No matching commands", systemImage: "magnifyingglass",
                    description: Text("Try another command or VM name."))
            }
        }
    }
}
