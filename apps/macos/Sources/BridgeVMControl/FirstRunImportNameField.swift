import SwiftUI

struct FirstRunImportNameField: View {
    @Binding var displayName: String

    // Match the existing import validator's required-name rule. File and
    // registration validation remain the import worker's responsibility.
    static func hasName(_ value: String) -> Bool {
        !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                TextField("VM 이름", text: $displayName)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.first-run.name")
                if !Self.hasName(displayName) {
                    Label(FirstRunImport.ValidationError.emptyName.description, systemImage: "exclamationmark.circle")
                        .font(.caption).foregroundStyle(.red)
                        .accessibilityIdentifier("bridgevm.first-run.name.error")
                }
            }
        } label: {
            Label("라이브러리에 표시할 이름", systemImage: "tag")
        }
    }
}
