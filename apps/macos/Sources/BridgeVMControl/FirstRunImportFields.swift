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
                    LibraryFileField(title: "Windows 디스크", detail: "Windows가 설치된 ARM64 디스크를 가져옵니다. RAW는 복사하고 QCOW2와 Hyper-V의 VHDX는 RAW로 변환합니다.", accessibilityIdentifier: "bridgevm.first-run.disk", path: $diskPath)
                    LibraryFileField(title: "부팅 설정 파일 (UEFI vars, 선택 사항)",
                        detail: HvfWindowsImportNotice.varsHelp, accessibilityIdentifier: "bridgevm.first-run.vars", path: $varsPath)
                    Text(HvfWindowsImportNotice.bitLockerHelp).font(.caption).foregroundColor(.secondary)
                    FirstRunVTPMImportFields(statePath: $vtpmPath, packagePath: $vtpmPackagePath,
                        codePath: $vtpmCodePath)
                }
            } label: {
                Label("가져올 파일", systemImage: "square.and.arrow.down")
            }
            GroupBox {
                HStack(spacing: 24) {
                    Stepper(value: $memGiB, in: 2...64) {
                        LibraryMetadataLabel(title: "메모리", value: "\(memGiB) GiB", symbol: "memorychip")
                    }
                    Divider().frame(height: 36)
                    Stepper(value: $cpuCount, in: 1...16) {
                        LibraryMetadataLabel(title: "CPU", value: "\(cpuCount) vCPU", symbol: "cpu")
                    }
                }
            } label: {
                Label("VM 구성", systemImage: "slider.horizontal.3")
            }
        }
    }
}
