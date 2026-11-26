import SwiftUI

struct FirstRunVTPMImportFields: View {
    @Binding var statePath: String
    @Binding var packagePath: String
    @Binding var codePath: String

    var body: some View {
        LibraryFileField(title: "vTPM state folder (optional)",
            detail: "To keep the original BridgeVM TPM, choose its state folder and both recovery files.",
            accessibilityIdentifier: "bridgevm.first-run.vtpm", path: $statePath, chooseDirectory: true)
        if FirstRunImportVTPMValidation.trimmed(statePath) != nil {
            LibraryFileField(title: "vTPM recovery package (required)", detail: "Choose the recovery package you exported from BridgeVM.",
                accessibilityIdentifier: "bridgevm.first-run.vtpm-package", path: $packagePath)
            LibraryFileField(title: "vTPM recovery code file (required)", detail: "Choose the private code file created with the package.",
                accessibilityIdentifier: "bridgevm.first-run.vtpm-code", path: $codePath)
        }
    }
}
