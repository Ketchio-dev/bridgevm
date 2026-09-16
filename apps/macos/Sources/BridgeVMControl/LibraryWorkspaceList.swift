import SwiftUI

struct LibraryWorkspaceList: View {
    @ObservedObject var library: LibraryModel
    var createIdentifier = "bridgevm.library.overview.create"
    @State private var query = ""

    private var matchingVMs: [VMConfig] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return library.vms }
        return library.vms.filter {
            $0.name.localizedStandardContains(term)
                || $0.displayName.localizedStandardContains(term)
                || $0.engineShortLabel.localizedStandardContains(term)
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if library.vms.isEmpty {
                emptyState
            } else {
                LibrarySearchField(query: $query)
                    .padding(.horizontal, 22).padding(.vertical, 16)
                ScrollView {
                    LazyVStack(spacing: 12) {
                        ForEach(matchingVMs) { config in
                            LibraryWorkspaceRow(
                                library: library,
                                config: config,
                                model: library.model(for: config),
                                selected: library.selectedID == config.slug && !library.proMode
                            )
                        }
                    }
                    .padding(.horizontal, 22).padding(.bottom, 22)
                }
            }
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle("가상 머신")
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("가상 머신").font(.system(size: 28, weight: .bold))
                    Text("\(library.vms.count)개의 VM")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Button {
                    library.proMode = false
                    library.selectedID = LibraryModel.firstRunImportSelectionID
                } label: {
                    Image(systemName: "square.and.arrow.down")
                }
                .controlSize(.large)
                .help("기존 VM 가져오기")
                .accessibilityLabel("가져오기")
                .accessibilityIdentifier("bridgevm.library.overview.import")
                Button { library.showingCreate = true } label: {
                    Label("새 VM", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .accessibilityIdentifier(createIdentifier)
            }
            if !library.libraryIssues.isEmpty {
                Label("확인이 필요한 라이브러리 항목 \(library.libraryIssues.count)개", systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.orange)
            }
        }
        .padding(22)
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("등록된 VM이 없습니다", systemImage: "desktopcomputer")
        } description: {
            Text("새 VM을 만들거나 기존 VM을 가져오세요.")
        } actions: {
            Button("새 VM 만들기") { library.showingCreate = true }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier(createIdentifier)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct LibraryWorkspaceRow: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    @ObservedObject var model: ControlModel
    let selected: Bool

    var body: some View {
        HStack(spacing: 8) {
            Button(action: select) {
                HStack(spacing: 16) {
                LibraryVMEmblem(engine: config.engineKind)
                VStack(alignment: .leading, spacing: 7) {
                    Text(config.name).font(.title3.weight(.semibold)).lineLimit(1)
                    HStack(spacing: 7) {
                        Circle().fill(stateColor).frame(width: 8, height: 8)
                        Text(stateText).foregroundStyle(stateColor)
                    }
                    .font(.callout.weight(.medium))
                    Text(resourceText)
                        .font(.callout).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(config.name), \(stateText), \(resourceText)")
            .accessibilityHint("VM 상세 정보 보기")
            Menu {
                LibraryVMContextMenu(library: library, config: config)
            } label: {
                Image(systemName: "ellipsis").frame(width: 30, height: 30)
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            .accessibilityLabel("\(config.name) 작업")
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(selected ? Color.blue.opacity(0.08) : LibraryAppearance.surface,
                    in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(selected ? Color.blue : Color.primary.opacity(0.09), lineWidth: selected ? 2 : 1)
                .allowsHitTesting(false)
        }
        .contextMenu { LibraryVMContextMenu(library: library, config: config) }
    }

    private func select() {
        library.proMode = false
        library.selectedID = config.slug
    }

    private var stateText: String {
        if config.installPending == true { return "설치 필요" }
        return model.running ? "실행 중" : "정지됨"
    }

    private var stateColor: Color {
        if config.installPending == true { return .orange }
        return model.running ? .green : .secondary
    }

    private var resourceText: String {
        let cpu = config.cpuCount.map { "\($0) vCPU" } ?? "CPU 자동"
        let memory = config.memMiB.map { $0.isMultiple(of: 1024) ? "\($0 / 1024) GiB RAM" : "\($0) MiB RAM" } ?? "메모리 자동"
        return "\(cpu)  ·  \(memory)  ·  \(config.engineShortLabel)"
    }
}
