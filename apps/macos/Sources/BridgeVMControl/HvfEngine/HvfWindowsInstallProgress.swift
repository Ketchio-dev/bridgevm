import Foundation

enum HvfWindowsInstallStage: Equatable {
    case idle
    case validating
    case preparingSource
    case installing
    case finalizing
    case cancelling
    case cancelled
    case done
    case failed(String)

    var label: String {
        switch self {
        case .idle: return "대기"
        case .validating: return "설치 입력 확인"
        case .preparingSource: return "설치 소스 준비"
        case .installing: return "Windows 무인 설치"
        case .finalizing: return "VM에 반영"
        case .cancelling: return "취소 중…"
        case .cancelled: return "취소됨"
        case .done: return "완료"
        case .failed: return "실패"
        }
    }

    var isRunning: Bool {
        switch self {
        case .validating, .preparingSource, .installing, .finalizing, .cancelling: return true
        default: return false
        }
    }
}
