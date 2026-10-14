import Foundation

extension HVFCreationOutcome {
    /// A collision remains a failed creation, while its prepared files survive
    /// independently of subsequent changes to the competing registration.
    static func publish(
        _ config: VMConfig, libraryRoot: URL, persist: Bool,
        preserving preserve: () -> Void = {},
        using publisher: (VMConfig, URL) -> VMRegistrationCommitOutcome
    ) -> HVFCreationOutcome? {
        guard persist else { return .created(config) }
        let outcome = publisher(config, libraryRoot)
        if outcome.preservesPreparedDestination { preserve() }
        switch outcome {
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
