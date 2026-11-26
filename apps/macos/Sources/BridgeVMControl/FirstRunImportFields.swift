import SwiftUI

struct FirstRunImportFields: View {
    @Binding var displayName: String
    @Binding var diskPath: String
    @Binding var varsPath: String
    @Binding var vtpmPath: String; @Binding var vtpmPackagePath: String; @Binding var vtpmCodePath: String
    @Binding var memGiB: Int
    @Binding var cpuCount: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            FirstRunImportNameField(displayName: $displayName)
            GroupBox {
                VStack(alignment: .leading, spacing: 20) {
                    LibraryFileField(title: "Windows disk", detail: "Choose a disk with Windows for Arm installed. BridgeVM makes a RAW copy of your RAW, QCOW2, fixed-size VHD, or VHDX disk.", accessibilityIdentifier: "bridgevm.first-run.disk", path: $diskPath)
                    LibraryFileField(title: "UEFI variables file (optional)",
                        detail: HvfWindowsImportNotice.varsHelp, accessibilityIdentifier: "bridgevm.first-run.vars", path: $varsPath)
                    Text(HvfWindowsImportNotice.bitLockerHelp).font(.caption).foregroundColor(.secondary)
                    FirstRunVTPMImportFields(statePath: $vtpmPath, packagePath: $vtpmPackagePath,
                        codePath: $vtpmCodePath)
                }
            } label: {
                Label("Files to import", systemImage: "square.and.arrow.down")
            }
            GroupBox {
                HStack(spacing: 24) {
                    Stepper(value: $memGiB, in: 2...64) {
                        LibraryMetadataLabel(title: "Memory", value: "\(memGiB) GiB", symbol: "memorychip")
                    }
                    Divider().frame(height: 36)
                    Stepper(value: $cpuCount, in: 1...16) {
                        LibraryMetadataLabel(title: "CPU", value: "\(cpuCount) vCPU", symbol: "cpu")
                    }
                }
            } label: {
                Label("VM resources", systemImage: "slider.horizontal.3")
            }
        }
    }
}
