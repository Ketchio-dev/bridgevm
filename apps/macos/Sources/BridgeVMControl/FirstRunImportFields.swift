import SwiftUI

struct FirstRunImportFields: View {
    @Binding var displayName: String
    @Binding var diskPath: String
    @Binding var varsPath: String
    @Binding var vtpmPath: String
    @Binding var memGiB: Int
    @Binding var cpuCount: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            FirstRunImportNameField(displayName: $displayName)
            GroupBox {
                VStack(alignment: .leading, spacing: 20) {
                    LibraryFileField(title: "Windows 디스크", detail: "이미 설치된 Windows ARM64 RAW 디스크를 선택하세요.", path: $diskPath)
                    LibraryFileField(title: "부팅 설정 파일 (UEFI vars)",
                        detail: "이 디스크를 부팅할 때 사용한 64 MiB UEFI vars 파일이 필요합니다.", path: $varsPath)
                    LibraryFileField(title: "vTPM 상태 폴더 (선택)",
                        detail: "기존 VM에 연결된 vTPM 상태가 있다면 해당 폴더를 선택하세요.", path: $vtpmPath, chooseDirectory: true)
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
