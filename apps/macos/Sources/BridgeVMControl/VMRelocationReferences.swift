import Foundation

enum VMRelocationReferences {
    static func permits(_ config: VMConfig, rootURL: URL) -> Bool {
        config.engineKind == .hvfEngine && refusal(for: config, rootURL: rootURL) == nil
    }

    static func refusal(for config: VMConfig, rootURL: URL) -> String? {
        do {
            try LibrarySavedReferences.validate(config, rootURL: rootURL,
                affectedPath: URL(fileURLWithPath: config.bundlePath))
            return nil
        } catch {
            return "다른 VM이 이동할 번들의 파일을 참조하거나 참조 여부를 확인할 수 없습니다. 관련 VM을 먼저 다른 위치로 옮기거나 등록 정보를 확인한 뒤 다시 이동하세요."
        }
    }

    static func failureMessage(for config: VMConfig, rootURL: URL) -> String {
        refusal(for: config, rootURL: rootURL)
            ?? "VM 이동 완료를 확인하지 못했습니다. 원본과 대상 위치의 번들 및 등록 상태를 확인하세요."
    }
}
