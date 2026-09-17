import SwiftUI
/// First launch offers creation/import and keeps accepted import recovery controls visible.
struct FirstRunView: View {
    @ObservedObject var library: LibraryModel

    @State private var displayName = "Windows 11"
    @State private var diskPath = ""; @State private var varsPath = ""; @State private var vtpmPath = ""
    @State private var vtpmPackagePath = ""; @State private var vtpmCodePath = ""
    @State private var memGiB = 6
    @State private var cpuCount = 4

    private var showsImport: Bool {
        library.selectedID == LibraryModel.firstRunImportSelectionID
            || library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if showsImport {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("기존 Windows VM 가져오기").font(.system(size: 28, weight: .bold))
                        Text("파일을 선택해 등록한 뒤 VM 상세 화면에서 시작하세요.").foregroundStyle(.secondary)
                    }
                    if !library.firstRunImportBusy && library.firstRunImport.publishedConfig == nil {
                        FirstRunImportFields(displayName: $displayName, diskPath: $diskPath, varsPath: $varsPath,
                            vtpmPath: $vtpmPath, vtpmPackagePath: $vtpmPackagePath, vtpmCodePath: $vtpmCodePath,
                            memGiB: $memGiB, cpuCount: $cpuCount)
                    }
                    FirstRunImportStatusView(library: library,
                        canImport: FirstRunImportNameField.hasName(displayName) && !diskPath.isEmpty && !varsPath.isEmpty, importAction: runImport)
                } else {
                    FirstRunWelcomeView(createAction: { library.showingCreate = true }, importAction: {
                        library.proMode = false
                        library.selectedID = LibraryModel.firstRunImportSelectionID
                    })
                }
            }
            .padding(28)
            .frame(maxWidth: 900, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle(showsImport ? "VM 가져오기" : "시작하기")
    }

    private func runImport() {
        guard !library.firstRunImportBusy else { return }
        let inputs = FirstRunImport.Inputs(
            displayName: displayName, diskPath: diskPath, varsPath: varsPath,
            vtpmStateDir: vtpmPath.isEmpty ? nil : vtpmPath,
            vtpmRecoveryPackagePath: vtpmPackagePath.isEmpty ? nil : vtpmPackagePath,
            vtpmRecoveryCodePath: vtpmCodePath.isEmpty ? nil : vtpmCodePath,
            memMiB: memGiB * 1024,
            cpuCount: cpuCount)
        Task { await library.importExistingHvfVM(inputs) }
    }
}
