import SwiftUI

struct FirstRunImportStatusView: View {
    @ObservedObject var library: LibraryModel
    let canImport: Bool
    let recoveryIssue: FirstRunImport.ValidationError?
    let importAction: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if library.firstRunImportBusy, let stage = library.firstRunImport.stage {
                VStack(alignment: .leading, spacing: 10) {
                    if let name = library.firstRunImport.displayName {
                        Text(name).font(.headline)
                    }
                    HStack(spacing: 8) {
                        if stage.conversionFraction == nil {
                            ProgressView().controlSize(.small).accessibilityHidden(true)
                        }
                        Text(stage.label).font(.callout).monospacedDigit()
                            .accessibilityIdentifier("bridgevm.first-run.import.stage")
                    }
                    if let fraction = stage.conversionFraction {
                        GeometryReader { track in
                            Capsule().fill(.quaternary)
                                .overlay(alignment: .leading) {
                                    Capsule().fill(.tint).frame(width: track.size.width * fraction)
                                }
                        }
                        .frame(height: 6)
                        .accessibilityHidden(true) // The stage text also carries the measured percent.
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
                .modifier(LibraryCardSurface())
            }
            if !library.firstRunImportBusy, library.firstRunImport.publishedConfig == nil, let recoveryIssue {
                Label(recoveryIssue.description, systemImage: "exclamationmark.circle")
                    .font(.caption).foregroundStyle(.red)
                    .accessibilityIdentifier("bridgevm.first-run.vtpm.required")
            } else if library.firstRunImport.savedWithNotice, let saved = library.firstRunImport.publishedConfig,
                      let notice = library.firstRunImportError {
                VStack(alignment: .leading, spacing: 10) {
                    Label("라이브러리에 저장됨", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.secondary).font(.subheadline.weight(.semibold))
                        .accessibilityIdentifier("bridgevm.first-run.import.saved")
                    Text(saved.displayName).font(.title3.weight(.semibold))
                        .accessibilityAddTraits(.isHeader)
                    Divider()
                    Label {
                        Text(notice).font(.callout)
                    } icon: {
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    }
                    .accessibilityIdentifier("bridgevm.first-run.import.error")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
                .modifier(LibraryCardSurface())
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
                    .keyboardShortcut(.defaultAction)
                    .disabled(library.firstRunImportBusy).accessibilityIdentifier("bridgevm.first-run.import.recover")
                } else if !library.firstRunImportBusy {
                    Button("가져오기", action: importAction)
                        .keyboardShortcut(.defaultAction).accessibilityIdentifier("bridgevm.first-run.import.commit")
                        .disabled(!canImport || recoveryIssue != nil)
                }
            }
            if library.firstRunImport.publishedConfig != nil, !library.firstRunImport.savedWithNotice {
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
