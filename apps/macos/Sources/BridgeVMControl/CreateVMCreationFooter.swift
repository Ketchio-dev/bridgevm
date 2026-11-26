import SwiftUI

struct CreateVMCreationFooter: View {
    let working: Bool
    let error: String
    let failureCode: String
    let canCreate: Bool
    let needsName: Bool
    let cancel: () -> Void
    let create: () -> Void
    let recoveryAvailable: Bool
    let recover: () -> Void
    /// What the wait is for, such as a QCOW2 conversion and its percent.
    var workingLabel = ""

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
                    Text(workingLabel.isEmpty ? "VM 파일 준비 중…" : workingLabel).font(.caption).foregroundStyle(.secondary)
                } else if needsName {
                    Text("생성하려면 VM 이름을 입력하세요.").font(.caption)
                        .accessibilityIdentifier("bridgevm.create.name.required")
                }
                Spacer()
                Button("취소") { if !working { cancel() } }
                    .keyboardShortcut(.cancelAction)
                    .disabled(working)
                    .accessibilityIdentifier("bridgevm.create.cancel")
                primaryAction
            }
            .padding(20)
        }
    }
}

extension CreateVMCreationFooter {
    var primaryAction: some View {
        Button(recoveryAvailable ? "저장된 VM 불러오기" : (working ? "생성 중…" : "생성"),
               action: recoveryAvailable ? recover : create)
            .keyboardShortcut(.defaultAction)
            .disabled(recoveryAvailable ? working : !canCreate)
            .accessibilityIdentifier(recoveryAvailable ? "bridgevm.create.recover" : "bridgevm.create.commit")
    }
}
