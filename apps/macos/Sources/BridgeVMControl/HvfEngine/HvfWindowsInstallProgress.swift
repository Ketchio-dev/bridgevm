import Foundation

enum HvfWindowsInstallStage: Equatable {
    case idle
    case validating
    case preparingSource
    case installing
    case finalizing
    case recovering
    case cancelling
    case cancelled
    case done
    case failed(String)

    var isRunning: Bool {
        switch self {
        case .validating, .preparingSource, .installing, .finalizing, .recovering, .cancelling: return true
        default: return false
        }
    }
}

extension HvfWindowsInstallStage {
    var label: String {
        switch self {
        case .idle: return "대기"
        case .validating: return "설치 입력 확인"
        case .preparingSource: return "설치 소스 준비"
        case .installing: return "Windows 무인 설치"
        case .finalizing: return "VM에 반영"
        case .recovering: return "기존 설치 결과 복구"
        case .cancelling: return "취소 중…"
        case .cancelled: return "취소됨"
        case .done: return "완료"
        case .failed: return "실패"
        }
    }

    var primaryActionTitle: String {
        switch self {
        case .failed, .cancelled: return "다시 시도"
        case .done: return "설치 완료"
        default: return "설치 시작"
        }
    }
}

extension HvfWindowsInstallStage {
    static let steps = ["파일 확인", "이미지 준비", "Windows 설치", "VM 저장"]

    var installationStep: Int? {
        switch self {
        case .validating: 0
        case .preparingSource: 1
        case .installing: 2
        case .finalizing: 3
        case .idle, .recovering, .cancelling, .cancelled, .done, .failed: nil
        }
    }
}
