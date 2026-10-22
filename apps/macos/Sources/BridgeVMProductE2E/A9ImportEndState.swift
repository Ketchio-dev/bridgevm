import Foundation

/// Failure-only summary of the first-run import screen and the lane library, appended when
/// the imported dashboard never appears (T19 r1 and r2 could not tell a stuck import from a
/// failed one). Product text never leaves the helper: stage labels and errors map to fixed
/// tokens, an error no rule knows is reported by its UTF-8 length only, and an OSStatus is
/// kept as a number.
enum A9ImportEndState {
    static let stage = "bridgevm.first-run.import.stage", error = "bridgevm.first-run.import.error"
    static let commit = "bridgevm.first-run.import.commit", recover = "bridgevm.first-run.import.recover"
    static let identifiers: Set<String> = [stage, error, commit, recover]

    static func capture(_ ui: T17UIControlling, request: A9ImportRequest, fileManager: FileManager) -> String {
        describe(try? ui.optionalTexts(identifiers),
                 registrations: A9LaneRegistrations.slugs(request: request, fileManager: fileManager), expected: request.vmSlug)
    }

    /// Appends the end state to a missing-element failure; every other error passes unchanged.
    static func attributed(_ endState: () -> String, _ body: () throws -> Void) throws {
        do { try body() } catch let blocker as T17Blocker where blocker.code == "ui-element-missing" {
            throw T17Blocker(code: blocker.code, detail: String("\(blocker.detail); \(endState())".prefix(1_024)))
        }
    }

    static func describe(_ texts: [String: String]?, registrations: [String]?, expected: String) -> String {
        let screen = texts.map {
            "stage=\(stageToken($0[stage])),error=\(errorToken($0[error])),commit=\($0[commit] != nil),recover=\($0[recover] != nil)"
        } ?? "screen=unreadable"
        let lane = registrations.map {
            "lane_registrations=\(min($0.count, 99)),expected_registered=\($0.contains(expected))"
        } ?? "lane_registrations=unreadable"
        return "import_state=v1,\(screen),\(lane)"
    }

    static func stageToken(_ label: String?) -> String {
        guard let label else { return "none" }
        return stages[label] ?? "other"
    }

    static func errorToken(_ message: String?) -> String {
        guard let message else { return "none" }
        let status = osStatus(in: message).map { "/status\($0)" } ?? ""
        if let outer = outcomes.first(where: { message.hasPrefix($0.prefix) }) {
            guard outer.token == "bundle-failed" else { return outer.token + status }
            return "bundle-failed/\(causes.first { message.contains($0.text) }?.token ?? "other")\(status)"
        }
        if validation.contains(where: { message.hasPrefix($0) }) { return "validation" }
        return "unclassified/bytes\(min(message.utf8.count, 99_999))"
    }

    private static func osStatus(in message: String) -> String? {
        guard let range = message.range(of: #"OSStatus -?[0-9]{1,10}"#, options: .regularExpression) else { return nil }
        return String(message[range].dropFirst("OSStatus ".count))
    }

    private static let stages = [
        "입력 파일 확인 중…": "validating", "VM 파일 복사 중…": "copying", "등록 정보 저장 중…": "saving",
        "라이브러리 확인 중…": "reading-library", "라이브러리에서 VM을 확인했습니다.": "complete",
    ]
    private static let outcomes: [(prefix: String, token: String)] = [
        ("VM 번들을 만들지 못했습니다:", "bundle-failed"), ("VM 등록 정보를 저장하지 못했습니다:", "registration-not-saved"),
        ("VM 파일과 등록 정보는 보존했지만", "published-unsynced"), ("VM은 저장했지만 라이브러리에서 찾지 못했습니다", "saved-not-listed"),
        ("저장된 VM을 아직 불러올 수 없습니다", "recovery-failed"), ("VM 가져오기가 이미 진행 중입니다", "already-running"),
        ("VM 가져오기를 시작하지 않았습니다", "not-started"),
    ]
    private static let causes: [(text: String, token: String)] = [
        ("대상 VM ID에 이미 vTPM 키가 있습니다", "destination-key-exists"), ("다른 Keychain 작업이 진행 중입니다", "keychain-busy"),
        ("사용자 확인이 필요합니다", "keychain-interaction"), ("Keychain에서 vTPM 상태 키를 읽지 못했습니다", "keychain-read"),
        ("Keychain에 vTPM 상태 키를 저장하지 못했습니다", "keychain-write"), ("기존 vTPM 상태의 Keychain 키가 없습니다", "missing-key"),
        ("vTPM 복구 패키지가 손상되었거나 코드와 일치하지 않습니다", "recovery-package"), ("vTPM 복구 파일을 찾을 수 없습니다", "recovery-file"),
        ("가져오기에 필요한 엔진 도구가 없거나 안전하지 않습니다", "engine-tool"),
    ]
    private static let validation = [
        "VM 이름을 입력하세요.", "디스크 이미지를 찾을 수 없습니다:", "디스크 경로가 파일이 아닙니다:", "디스크 이미지가 비어 있습니다:",
        "UEFI vars 파일", "vTPM 상태 경로가 디렉터리가 아닙니다:", "vTPM 복구 파일을 찾을 수 없습니다:", "vTPM 복구 파일을 사용하려면",
        "RAM은 2048 MiB 이상",
    ]
}
