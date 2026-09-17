import SwiftUI

struct FirstRunImportStatusView: View {
    @ObservedObject var library: LibraryModel
    let canImport: Bool
    let importAction: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if library.firstRunImportBusy, let stage = library.firstRunImport.stage {
                HStack {
                    ProgressView().controlSize(.small)
                    Text(stage.label).font(.callout).accessibilityIdentifier("bridgevm.first-run.import.stage")
                }
            }
            if let error = library.firstRunImportError {
                Text(error).foregroundStyle(.red).font(.callout)
            }
            HStack {
                Spacer()
                if library.firstRunImport.publishedConfig != nil {
                    Button("다른 VM 가져오기") { library.returnToFirstRunInputs() }
                        .disabled(library.firstRunImportBusy)
                    Button("저장된 VM 불러오기") {
                        Task { await library.recoverPublishedFirstRunImport() }
                    }
                    .disabled(library.firstRunImportBusy)
                } else {
                    Button(library.firstRunImportBusy ? "가져오는 중…" : "가져오기", action: importAction)
                        .keyboardShortcut(.defaultAction).accessibilityIdentifier("bridgevm.first-run.import.commit")
                        .disabled(library.firstRunImportBusy || !canImport)
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
