import SwiftUI

// Preserve the existing overview route identity used by the native routing tests.
struct FleetTableView: View {
    @ObservedObject var library: LibraryModel
    var body: some View { LibraryOverviewView(library: library) }
}

struct LibraryOverviewView: View {
    @ObservedObject var library: LibraryModel
    var createIdentifier = "bridgevm.library.overview.create"
    @State private var query = ""

    private var matchingVMs: [VMConfig] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return library.vms }
        return library.vms.filter {
            $0.name.localizedStandardContains(term) || $0.engineShortLabel.localizedStandardContains(term)
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                HStack(alignment: .top, spacing: 20) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("가상 머신").font(.system(size: 30, weight: .bold))
                        Text("VM을 선택하고 설치, 실행, 설정을 이어가세요.")
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Button {
                        library.proMode = false
                        library.selectedID = LibraryModel.firstRunImportSelectionID
                    } label: {
                        Label("가져오기", systemImage: "square.and.arrow.down")
                    }
                    .controlSize(.large)
                    .accessibilityIdentifier("bridgevm.library.overview.import")
                    Button { library.showingCreate = true } label: {
                        Label("새 VM 만들기", systemImage: "plus")
                    }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .accessibilityIdentifier(createIdentifier)
                }
                HStack(spacing: 24) {
                    LibraryMetadataLabel(title: "등록된 VM", value: "\(library.vms.count)개", symbol: "square.stack.3d.up")
                    Divider().frame(height: 32)
                    LibraryMetadataLabel(title: "HVF 엔진", value: "\(library.vms.filter { $0.engineKind == .hvfEngine }.count)개", symbol: "cpu")
                    Divider().frame(height: 32)
                    LibraryMetadataLabel(title: "설치 필요", value: "\(library.vms.filter { $0.installPending == true }.count)개", symbol: "arrow.down.circle")
                    Spacer(minLength: 0)
                }
                .padding(20).modifier(LibraryCardSurface())
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
                    TextField("VM 이름 또는 엔진 검색", text: $query)
                        .textFieldStyle(.plain)
                        .accessibilityIdentifier("bridgevm.library.search")
                    if !query.isEmpty {
                        Button { query = "" } label: { Image(systemName: "xmark.circle.fill") }
                            .buttonStyle(.plain).foregroundStyle(.secondary)
                            .accessibilityLabel("검색 지우기")
                    }
                }
                .padding(12)
                .background(LibraryAppearance.inset, in: RoundedRectangle(cornerRadius: 10))
                if matchingVMs.isEmpty {
                    ContentUnavailableView {
                        Label(query.isEmpty ? "등록된 VM이 없습니다" : "검색 결과가 없습니다", systemImage: "desktopcomputer")
                    } description: {
                        Text(query.isEmpty ? "새 VM을 만들거나 위의 가져오기로 기존 VM을 등록하세요." : "다른 이름이나 엔진으로 검색해 보세요.")
                    }
                    .frame(maxWidth: .infinity, minHeight: 240)
                } else {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 18)], spacing: 18) {
                        ForEach(matchingVMs) { config in
                            LibraryVMCard(library: library, config: config)
                        }
                    }
                }
                Text("카드에는 등록 정보와 저장된 구성이 표시됩니다. 실행 상태는 VM 상세 화면에서 확인할 수 있습니다.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .padding(28)
            .frame(maxWidth: 1400, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle("가상 머신")
    }
}
