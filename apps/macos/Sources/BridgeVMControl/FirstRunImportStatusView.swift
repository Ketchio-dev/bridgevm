import SwiftUI

struct FirstRunImportStatusView: View {
    @ObservedObject var library: LibraryModel
    let canImport: Bool
    let recoveryIssue: FirstRunImport.ValidationError?
    let importAction: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if library.firstRunImportBusy, let stage = library.firstRunImport.stage {
                HStack {
                    ProgressView().controlSize(.small)
                    Text(stage.label).font(.callout).accessibilityIdentifier("bridgevm.first-run.import.stage")
                }
            }
            if !library.firstRunImportBusy, library.firstRunImport.publishedConfig == nil, let recoveryIssue {
                Label(recoveryIssue.description, systemImage: "exclamationmark.circle")
                    .font(.caption).foregroundStyle(.red)
                    .accessibilityIdentifier("bridgevm.first-run.vtpm.required")
            } else if let error = library.firstRunImportError {
                Text(error).foregroundStyle(.red).font(.callout).accessibilityIdentifier("bridgevm.first-run.import.error")
            }
            VMPreparationQuitStatus(library: library)
            HStack {
                Spacer()
                if library.firstRunImport.publishedConfig != nil {
                    Button("다른 VM 가져오기") { library.returnToFirstRunInputs() }
                        .disabled(library.firstRunImportBusy)
                    Button("저장된 VM 불러오기") {
                        Task { await library.recoverPublishedFirstRunImport() }
                    }
                    .disabled(library.firstRunImportBusy).accessibilityIdentifier("bridgevm.first-run.import.recover")
                } else {
                    Button(library.firstRunImportBusy ? "가져오는 중…" : "가져오기", action: importAction)
                        .keyboardShortcut(.defaultAction).accessibilityIdentifier("bridgevm.first-run.import.commit")
                        .disabled(library.firstRunImportBusy || !canImport || recoveryIssue != nil)
                }
            }
            if library.firstRunImport.publishedConfig != nil {
                Text("저장된 VM은 다시 복사하지 않고 불러옵니다. 다른 VM을 가져와도 기존 파일은 남습니다. 새 VM은 다른 이름으로 가져오세요.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: 620)
    }
}

struct VMPreparationQuitStatus: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if library.quitAfterVMPreparation?.confirmed == true {
            HStack {
                Text("VM 파일 준비를 마치면 BridgeVM을 종료합니다.")
                Button("종료 취소") { library.quitAfterVMPreparation = nil }
            }
            .font(.callout)
        }
    }
}
