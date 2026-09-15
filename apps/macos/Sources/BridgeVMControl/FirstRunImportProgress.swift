import Foundation

enum FirstRunImportStage: Int, Sendable {
    case validating, copying, saving, readingLibrary, complete
    var label: String {
        switch self {
        case .validating: return "입력 파일 확인 중…"
        case .copying: return "VM 파일 복사 중…"
        case .saving: return "등록 정보 저장 중…"
        case .readingLibrary: return "라이브러리 확인 중…"
        case .complete: return "라이브러리에서 VM을 확인했습니다."
        }
    }
}

enum FirstRunImportResult: Sendable {
    case committed(VMConfig)
    case failed(String)
    case publishedButUnsynced(VMConfig, String)
}

typealias FirstRunImportOperation = @Sendable (FirstRunImport.Inputs, URL, URL,
    @escaping @Sendable (FirstRunImportStage) -> Void) -> FirstRunImportResult

struct FirstRunImportProgress {
    private(set) var operationID: UUID?
    private(set) var stage: FirstRunImportStage?
    private(set) var error: String?
    private(set) var publishedConfig: VMConfig?
    private var publicationWarning: String?
    var isBusy: Bool { operationID != nil }

    mutating func begin() -> UUID {
        error = nil
        stage = .validating
        let id = UUID()
        operationID = id
        return id
    }

    mutating func beginRecovery() {
        operationID = UUID()
        stage = .readingLibrary
        // Keep the publication warning visible until explicit read-back succeeds.
    }

    mutating func advance(_ next: FirstRunImportStage, operation: UUID) {
        guard operationID == operation, next.rawValue >= (stage?.rawValue ?? -1) else { return }
        stage = next
    }

    mutating func finish(error: String?, publishedConfig: VMConfig? = nil) {
        operationID = nil
        self.error = error
        self.publishedConfig = publishedConfig
        publicationWarning = publishedConfig == nil ? nil : error
        if error == nil { stage = .complete }
    }

    mutating func recoveryFailed(_ message: String) {
        operationID = nil
        error = [publicationWarning, message].compactMap { $0 }.joined(separator: "\n")
    }

    mutating func returnToInputs() { self = FirstRunImportProgress() }
}
