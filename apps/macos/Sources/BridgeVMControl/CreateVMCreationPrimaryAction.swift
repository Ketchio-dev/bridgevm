import SwiftUI

extension CreateVMCreationFooter {
    var primaryAction: some View {
        Button(recoveryAvailable ? "저장된 VM 불러오기" : (working ? "생성 중…" : "생성"),
               action: recoveryAvailable ? recover : create)
            .keyboardShortcut(.defaultAction)
            .disabled(recoveryAvailable ? working : !canCreate)
            .accessibilityIdentifier(recoveryAvailable ? "bridgevm.create.recover" : "bridgevm.create.commit")
    }
}
