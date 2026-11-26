import SwiftUI
/// First launch offers creation/import and keeps accepted import recovery controls visible.
struct FirstRunView: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.libraryWindowPresentation) private var presentation

    @Environment(\.firstRunFormDraft) private var windowDraft
    @State private var standaloneDraft = FirstRunFormDraft()

    private var showsImport: Bool {
        presentation?.wrappedValue.showsImportDraft == true || library.selectedID == LibraryModel.firstRunImportSelectionID
            || library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
    }

    var body: some View {
        let draft = windowDraft ?? $standaloneDraft
        Group {
            if showsImport {
                let showsInputs = !library.firstRunImportBusy && library.firstRunImport.publishedConfig == nil
                let status = FirstRunImportStatusView(library: library,
                    canImport: FirstRunImportNameField.hasName(draft.wrappedValue.displayName) && !draft.wrappedValue.diskPath.isEmpty,
                    recoveryIssue: recoveryIssue(draft.wrappedValue), importAction: { runImport(draft.wrappedValue) })
                VStack(spacing: 0) {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 24) {
                            VStack(alignment: .leading, spacing: 8) {
                                if let presentation, presentation.wrappedValue.showsImportDraft {
                                    Button { presentation.wrappedValue.showsImportDraft = false } label: {
                                        Label("Back to Get Started", systemImage: "chevron.backward")
                                    }
                                    .buttonStyle(.link)
                                    .keyboardShortcut(.cancelAction)
                                    .accessibilityIdentifier("bridgevm.first-run.back")
                                }
                                Text("Import a Windows VM").font(.system(size: 28, weight: .bold))
                                if showsInputs {
                                    Text("Choose your installed Windows disk. After import, open the VM in your library to start Windows.").foregroundStyle(.secondary)
                                }
                            }
                            if showsInputs {
                                FirstRunImportFields(displayName: draft.displayName, diskPath: draft.diskPath, varsPath: draft.varsPath,
                                    vtpmPath: draft.vtpmPath, vtpmPackagePath: draft.vtpmPackagePath, vtpmCodePath: draft.vtpmCodePath,
                                    memGiB: draft.memGiB, cpuCount: draft.cpuCount)
                            } else {
                                status.frame(maxWidth: 620)
                            }
                        }
                        .padding(28)
                        .frame(maxWidth: 900, alignment: .leading)
                        .frame(maxWidth: .infinity)
                    }
                    if showsInputs {
                        Divider()
                        status
                            .padding(.horizontal, 28)
                            .padding(.vertical, 12)
                            .frame(maxWidth: 900)
                            .frame(maxWidth: .infinity)
                            .background(.bar)
                    }
                }
            } else {
                welcome(in: presentation, diskPath: draft.diskPath)
            }
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle(showsImport ? "Import VM" : "Get Started")
    }

    func welcome(in presentation: Binding<LibraryWindowPresentation>?, diskPath: Binding<String>) -> FirstRunWelcomeView {
        FirstRunWelcomeView(createAction: { presentation?.wrappedValue.sheet = .create(installer: nil) },
                            importAction: { presentation?.wrappedValue.showsImportDraft = true }, dropAction: { choice, url in
            guard let presentation else { return false }
            switch choice {
            case .installer:
                presentation.wrappedValue.sheet = .create(installer: url)
            case .disk:
                diskPath.wrappedValue = url.path
                presentation.wrappedValue.showsImportDraft = true
            }
            return true
        })
    }

    private func recoveryIssue(_ draft: FirstRunFormDraft) -> FirstRunImport.ValidationError? {
        guard FirstRunImportVTPMValidation.trimmed(draft.vtpmPath) != nil,
              case .failure(let error) = FirstRunImportVTPMValidation.recoveryFiles(
                package: draft.vtpmPackagePath, code: draft.vtpmCodePath) else { return nil }
        return error
    }

    private func runImport(_ draft: FirstRunFormDraft) {
        guard !library.firstRunImportBusy else { return }
        if let presentation, presentation.wrappedValue.showsImportDraft {
            library.proMode = false
            library.selectedID = LibraryModel.firstRunImportSelectionID
            presentation.wrappedValue.showsImportDraft = false
        }
        let state = FirstRunImportVTPMValidation.trimmed(draft.vtpmPath)
        let inputs = FirstRunImport.Inputs(
            displayName: draft.displayName, diskPath: draft.diskPath, varsPath: draft.varsPath,
            vtpmStateDir: state, vtpmRecoveryPackagePath: state == nil ? nil : draft.vtpmPackagePath,
            vtpmRecoveryCodePath: state == nil ? nil : draft.vtpmCodePath,
            memMiB: draft.memGiB * 1024, cpuCount: draft.cpuCount)
        Task { await library.importExistingHvfVM(inputs) }
    }
}
