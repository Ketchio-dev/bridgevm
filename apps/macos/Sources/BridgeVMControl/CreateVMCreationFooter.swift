import SwiftUI

struct CreateVMCreationFooter: View {
    let working: Bool
    let error: String
    let failureCode: String
    let canCreate: Bool
    let cancel: () -> Void
    let create: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !error.isEmpty {
                Text(error).font(.caption).foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("bridgevm.create.error").accessibilityValue(failureCode)
                    .padding(.horizontal, 20).padding(.top, 10)
            }
            HStack(spacing: 10) {
                if working {
                    ProgressView().controlSize(.small)
                    Text("VM 파일 준비 중…").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("취소") { if !working { cancel() } }
                    .keyboardShortcut(.cancelAction)
                    .disabled(working)
                    .accessibilityIdentifier("bridgevm.create.cancel")
                Button(working ? "생성 중…" : "생성", action: create)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canCreate)
                    .accessibilityIdentifier("bridgevm.create.commit")
            }
            .padding(20)
        }
    }
}
