import SwiftUI
/// First launch offers creation/import and keeps accepted import recovery controls visible.
struct FirstRunView: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.libraryWindowPresentation) private var presentation

    @State private var displayName = "Windows 11"
    @State private var diskPath = ""; @State private var varsPath = ""; @State private var vtpmPath = ""
    @State private var vtpmPackagePath = ""; @State private var vtpmCodePath = ""
    @State private var memGiB = CreateVMMemoryOptions.current.recommendedMiB / 1024
    @State private var cpuCount = 4

    private var showsImport: Bool {
        presentation?.wrappedValue.preparingDisk == true || library.selectedID == LibraryModel.firstRunImportSelectionID
            || library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
    }

    var body: some View {
        Group {
            if showsImport {
                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        VStack(alignment: .leading, spacing: 8) {
                            if let presentation, presentation.wrappedValue.preparingDisk {
                                Button { presentation.wrappedValue.preparingDisk = false } label: {
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
                welcome(in: presentation, diskPath: $diskPath)
            }
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle(showsImport ? "VM 가져오기" : "시작하기")
    }

    func welcome(in presentation: Binding<LibraryWindowPresentation>?, diskPath: Binding<String>) -> FirstRunWelcomeView {
        FirstRunWelcomeView(createAction: { presentation?.wrappedValue.sheet = .create(installer: nil) },
                            importAction: showImport, dropAction: { choice, url in
            guard let presentation else { return false }
            switch choice {
            case .installer:
                presentation.wrappedValue.sheet = .create(installer: url)
            case .disk:
                diskPath.wrappedValue = url.path
                presentation.wrappedValue.preparingDisk = true
            }
            return true
        })
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
        if let presentation, presentation.wrappedValue.preparingDisk {
            showImport()
            presentation.wrappedValue.preparingDisk = false
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
