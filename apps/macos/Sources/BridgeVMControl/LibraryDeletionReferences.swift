import Foundation

/// All product deletion paths remove an entire library entry, which can also
/// contain a different saved VM's relocated bundle or installation inputs.
/// Check immediately before removal; this is not a concurrent-writer lock.
enum LibraryDeletionReferences {
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        guard refusal(for: config, rootURL: rootURL) == nil else { return false }
        return VMLibrary.delete(config.slug, rootURL: rootURL)
    }

    static func refusal(for config: VMConfig, rootURL: URL) -> String? {
        do {
            try HvfPendingDeletionReferences.validate(config, rootURL: rootURL)
            return nil
        } catch {
            return "다른 VM이 이 항목의 파일을 참조하거나 참조 여부를 확인할 수 없습니다. 관련 VM을 다른 위치로 옮기거나 등록 정보를 확인한 뒤 다시 삭제하세요."
        }
    }

    static func failureMessage(for config: VMConfig, rootURL: URL) -> String {
        refusal(for: config, rootURL: rootURL)
            ?? "\(config.name)의 라이브러리 항목을 디스크에서 삭제하지 못했습니다."
    }
}
