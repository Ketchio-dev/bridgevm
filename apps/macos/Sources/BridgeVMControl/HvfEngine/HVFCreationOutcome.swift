import Foundation

enum HVFCreationOutcome: Sendable {
    case created(VMConfig)
    case publishedButUnsynced(VMConfig, String)

    /// A non-nil outcome transfers ownership of the prepared bundle to the caller,
    /// even when publication succeeded but its directory sync did not.
    static func publish(
        _ config: VMConfig, libraryRoot: URL, persist: Bool,
        using publisher: (VMConfig, URL) -> VMRegistrationCommitOutcome
    ) -> HVFCreationOutcome? {
        guard persist else { return .created(config) }
        switch publisher(config, libraryRoot) {
        case .committed:
            return .created(config)
        case .notPublished:
            return nil
        case .publishedButUnsynced:
            return .publishedButUnsynced(config,
                "VM 파일과 등록 정보는 보존했지만 저장 완료를 확인하지 못했습니다. 다시 만들지 말고 라이브러리를 다시 확인하세요.")
        }
    }
}
