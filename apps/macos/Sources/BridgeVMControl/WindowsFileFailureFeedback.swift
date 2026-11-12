import Foundation

/// Presentation never exposes error type names, paths or implementation details.
/// Keep both the reflected error and Foundation's description in diagnostics.
enum WindowsFileFailureFeedback {
    static let preparation = "Windows 설치 파일을 준비하지 못했으니 BridgeVM을 다시 설치하고 VM 폴더의 접근 권한과 여유 공간을 확인한 뒤 다시 시도하세요."
    static let source = "Windows 설치 파일을 사용할 수 없으니 다른 설치 작업이 끝났는지와 VM 폴더의 접근 권한을 확인하고 다시 시도하세요."
    static let finalization = "Windows 설치 결과를 저장하지 못했으니 VM 파일을 지우지 말고 저장 장치의 연결과 여유 공간을 확인한 뒤 다시 시도하세요."
    static let recovery = "중단된 Windows 설치를 복구하지 못했으니 VM 파일을 보존하고 저장 장치의 연결과 접근 권한을 확인한 뒤 다시 시도하세요."
    static let importing = "Windows 파일을 가져오지 못했으니 디스크와 부팅 설정 파일의 접근 권한과 저장 장치의 여유 공간을 확인하고 다시 시도하세요."
    static let publication = "VM 등록 정보를 저장하지 못했으니 라이브러리 폴더의 접근 권한과 여유 공간을 확인하고 다시 시도하세요."
    static let unsynced = "VM 파일은 보존했지만 저장 완료를 확인하지 못했으니 라이브러리에서 VM 등록 상태를 확인하세요."

    static func diagnostic(_ stage: String, _ error: Error) -> String {
        "Windows file operation failed [\(stage)]: \(String(reflecting: error)); \(error.localizedDescription)"
    }

    static func report(_ stage: String, _ error: Error, sentence: String) -> String {
        NSLog("%@", diagnostic(stage, error))
        // These converter refusals already give useful remediation; retain it without
        // the helper's appended format/version details or English parser diagnostics.
        let ns = error as NSError
        if ns.domain == "BridgeVM.MediaImport" {
            let detail = ns.localizedDescription
            if detail.hasPrefix("이 디스크는 다른 디스크를 기반으로") {
                return "이 디스크는 다른 디스크를 기반으로 하니 원래 VM 앱에서 하나의 디스크로 합친 뒤 다시 가져오세요."
            }
            if detail.hasPrefix("이 디스크는 지원하지 않는 형식 기능") {
                return "지원하지 않는 디스크 형식이므로 원래 VM 앱에서 디스크를 변환한 뒤 다시 가져오세요."
            }
            if detail.hasPrefix("디스크 이미지가 손상되었거나 구조가 올바르지") {
                return "디스크 이미지가 손상되었거나 구조가 올바르지 않으니 원래 VM 앱에서 디스크를 점검한 뒤 다시 시도하세요."
            }
        }
        return sentence
    }
}
