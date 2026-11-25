import SwiftUI
/// First launch offers creation/import and keeps accepted import recovery controls visible.
struct FirstRunView: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.firstRunWindowMedia) private var welcomeMedia

    @State private var displayName = "Windows 11"
    @State private var diskPath = ""; @State private var varsPath = ""; @State private var vtpmPath = ""
    @State private var vtpmPackagePath = ""; @State private var vtpmCodePath = ""
    @State private var memGiB = CreateVMMemoryOptions.current.recommendedMiB / 1024
    @State private var cpuCount = 4

    private var showsImport: Bool {
        welcomeMedia?.wrappedValue.preparingDisk == true || library.selectedID == LibraryModel.firstRunImportSelectionID
            || library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
    }

    var body: some View {
        Group {
            if showsImport {
                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        VStack(alignment: .leading, spacing: 8) {
                            if let welcomeMedia, welcomeMedia.wrappedValue.preparingDisk {
                                Button { welcomeMedia.wrappedValue.preparingDisk = false } label: {
                                    Label("시작 화면으로", systemImage: "chevron.backward")
                                }
                                .buttonStyle(.link)
                            }
                            Text("기존 Windows VM 가져오기").font(.system(size: 28, weight: .bold))
                            Text("파일을 선택해 등록한 뒤 VM 상세 화면에서 시작하세요.").foregroundStyle(.secondary)
                        }
                        if !library.firstRunImportBusy && library.firstRunImport.publishedConfig == nil {
                            FirstRunImportFields(displayName: $displayName, diskPath: $diskPath, varsPath: $varsPath,
                                vtpmPath: $vtpmPath, vtpmPackagePath: $vtpmPackagePath, vtpmCodePath: $vtpmCodePath,
                                memGiB: $memGiB, cpuCount: $cpuCount)
                        }
                        FirstRunImportStatusView(library: library,
                            canImport: FirstRunImportNameField.hasName(displayName) && !diskPath.isEmpty,
                            recoveryIssue: recoveryIssue, importAction: runImport)
                    }
                    .padding(28)
                    .frame(maxWidth: 900, alignment: .leading)
                    .frame(maxWidth: .infinity)
                }
            } else {
                FirstRunWelcomeView(createAction: { library.showingCreate = true }, importAction: showImport,
                                    dropAction: { choice, url in
                    guard let welcomeMedia else { return false }
                    switch choice {
                    case .installer:
                        welcomeMedia.wrappedValue.installer = FirstRunInstallerMedia(url: url)
                    case .disk:
                        diskPath = url.path
                        welcomeMedia.wrappedValue.preparingDisk = true
                    }
                    return true
                })
            }
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle(showsImport ? "VM 가져오기" : "시작하기")
    }

    private func showImport() {
        library.proMode = false
        library.selectedID = LibraryModel.firstRunImportSelectionID
    }

    private var recoveryIssue: FirstRunImport.ValidationError? {
        guard FirstRunImportVTPMValidation.trimmed(vtpmPath) != nil,
              case .failure(let error) = FirstRunImportVTPMValidation.recoveryFiles(
                package: vtpmPackagePath, code: vtpmCodePath) else { return nil }
        return error
    }

    private func runImport() {
        guard !library.firstRunImportBusy else { return }
        if let welcomeMedia, welcomeMedia.wrappedValue.preparingDisk {
            showImport()
            welcomeMedia.wrappedValue.preparingDisk = false
        }
        let state = FirstRunImportVTPMValidation.trimmed(vtpmPath)
        let inputs = FirstRunImport.Inputs(
            displayName: displayName, diskPath: diskPath, varsPath: varsPath,
            vtpmStateDir: state, vtpmRecoveryPackagePath: state == nil ? nil : vtpmPackagePath,
            vtpmRecoveryCodePath: state == nil ? nil : vtpmCodePath,
            memMiB: memGiB * 1024,
            cpuCount: cpuCount)
        Task { await library.importExistingHvfVM(inputs) }
    }
}
