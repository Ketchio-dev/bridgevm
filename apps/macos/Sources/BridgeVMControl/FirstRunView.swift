import SwiftUI
/// First launch offers creation/import and keeps accepted import recovery controls visible.
struct FirstRunView: View {
    @ObservedObject var library: LibraryModel

    @Environment(\.firstRunFormDraft) private var windowDraft
    @State private var standaloneDraft = FirstRunFormDraft()

    private var showsImport: Bool {
        library.selectedID == LibraryModel.firstRunImportSelectionID
            || library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
    }

    var body: some View {
        let draft = windowDraft ?? $standaloneDraft
        Group {
            if showsImport {
                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("기존 Windows VM 가져오기").font(.system(size: 28, weight: .bold))
                            Text("파일을 선택해 등록한 뒤 VM 상세 화면에서 시작하세요.").foregroundStyle(.secondary)
                        }
                        if !library.firstRunImportBusy && library.firstRunImport.publishedConfig == nil {
                            FirstRunImportFields(displayName: draft.displayName, diskPath: draft.diskPath, varsPath: draft.varsPath,
                                vtpmPath: draft.vtpmPath, vtpmPackagePath: draft.vtpmPackagePath, vtpmCodePath: draft.vtpmCodePath,
                                memGiB: draft.memGiB, cpuCount: draft.cpuCount)
                        }
                        FirstRunImportStatusView(library: library,
                            canImport: FirstRunImportNameField.hasName(draft.wrappedValue.displayName) && !draft.wrappedValue.diskPath.isEmpty,
                            recoveryIssue: recoveryIssue(draft.wrappedValue), importAction: { runImport(draft.wrappedValue) })
                    }
                    .padding(28)
                    .frame(maxWidth: 900, alignment: .leading)
                    .frame(maxWidth: .infinity)
                }
            } else {
                FirstRunWelcomeView(createAction: { library.showingCreate = true }, importAction: {
                    library.proMode = false
                    library.selectedID = LibraryModel.firstRunImportSelectionID
                })
            }
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle(showsImport ? "VM 가져오기" : "시작하기")
    }

    private func recoveryIssue(_ draft: FirstRunFormDraft) -> FirstRunImport.ValidationError? {
        guard FirstRunImportVTPMValidation.trimmed(draft.vtpmPath) != nil,
              case .failure(let error) = FirstRunImportVTPMValidation.recoveryFiles(
                package: draft.vtpmPackagePath, code: draft.vtpmCodePath) else { return nil }
        return error
    }

    private func runImport(_ draft: FirstRunFormDraft) {
        guard !library.firstRunImportBusy else { return }
        let state = FirstRunImportVTPMValidation.trimmed(draft.vtpmPath)
        let inputs = FirstRunImport.Inputs(
            displayName: draft.displayName, diskPath: draft.diskPath, varsPath: draft.varsPath,
            vtpmStateDir: state, vtpmRecoveryPackagePath: state == nil ? nil : draft.vtpmPackagePath,
            vtpmRecoveryCodePath: state == nil ? nil : draft.vtpmCodePath,
            memMiB: draft.memGiB * 1024, cpuCount: draft.cpuCount)
        Task { await library.importExistingHvfVM(inputs) }
    }
}
