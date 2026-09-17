import SwiftUI

struct FirstRunVTPMImportFields: View {
    @Binding var statePath: String
    @Binding var packagePath: String
    @Binding var codePath: String

    var body: some View {
        LibraryFileField(title: "vTPM 상태 폴더 (선택)",
            detail: "기존 TPM을 유지하려면 상태와 복구 파일을 함께 선택하세요.",
            accessibilityIdentifier: "bridgevm.first-run.vtpm", path: $statePath, chooseDirectory: true)
        if !statePath.isEmpty {
            LibraryFileField(title: "vTPM 복구 패키지", detail: "BridgeVM에서 내보낸 복구 패키지입니다.",
                accessibilityIdentifier: "bridgevm.first-run.vtpm-package", path: $packagePath)
            LibraryFileField(title: "vTPM 복구 코드 파일", detail: "패키지와 함께 생성된 비공개 코드 파일입니다.",
                accessibilityIdentifier: "bridgevm.first-run.vtpm-code", path: $codePath)
        }
    }
}
