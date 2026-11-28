import SwiftUI

struct CreateVMCreationFooter: View {
    let working: Bool
    let error: String
    let failureCode: String
    let canCreate: Bool
    /// What Create still needs (CreateVMSheet.footerHint).
    let hint: String?
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
                    .accessibilityIdentifier(Self.errorIdentifier(failureCode))
                    .padding(.horizontal, 20).padding(.top, 10)
            }
            HStack(spacing: 10) {
                if working {
                    ProgressView().controlSize(.small)
                    Text(workingLabel.isEmpty ? "Preparing VM files…" : workingLabel).font(.caption).foregroundStyle(.secondary)
                } else if let hint {
                    Text(hint).font(.caption)
                        .accessibilityIdentifier(hint == Self.nameHint ? "bridgevm.create.name.required" : "bridgevm.create.hint")
                }
                Spacer()
                Button("Cancel") { if !working { cancel() } }
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
    static let nameHint = "Enter a name to create this VM."

    /// The end-to-end harness reads the failure code from the identifier. VoiceOver never
    /// reads identifiers, so it reads only the message.
    static func errorIdentifier(_ code: String) -> String {
        code.isEmpty ? "bridgevm.create.error" : "bridgevm.create.error.\(code)"
    }

    var primaryAction: some View {
        Button(recoveryAvailable ? "Load Saved VM" : (working ? "Creating…" : "Create"),
               action: recoveryAvailable ? recover : create)
            .keyboardShortcut(.defaultAction)
            .disabled(recoveryAvailable ? working : !canCreate)
            .accessibilityIdentifier(recoveryAvailable ? "bridgevm.create.recover" : "bridgevm.create.commit")
    }
}
