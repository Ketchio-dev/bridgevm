import Foundation

enum FirstRunImportResult: Sendable {
    case committed(VMConfig)
    case failed(String)
    case publishedButUnsynced(VMConfig, String)
}

enum FirstRunImportWorker {
    /// Runs entirely on the worker. Ownership is relinquished only after the
    /// configuration is published, including an uncertain parent-directory sync.
    static func run(
        _ inputs: FirstRunImport.Inputs, libraryRoot: URL,
        snapshotHelper: URL = HvfMediaImportHelper.bundled,
        publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { VMLibrary.saveOutcome($0, rootURL: $1) }
    ) -> FirstRunImportResult {
        if let error = FirstRunImport.validate(inputs) { return .failed(error.description) }
        do {
            let prepared = try FirstRunImport.prepare(inputs, slug: VMConfig.slugify(inputs.displayName),
                libraryRoot: libraryRoot, snapshotHelper: snapshotHelper)
            switch publish(prepared.config, libraryRoot) {
            case .committed:
                prepared.preserve()
                return .committed(prepared.config)
            case .notPublished(let error):
                return .failed("VM 등록 정보를 저장하지 못했습니다: \(error.localizedDescription)")
            case .publishedButUnsynced(let error):
                prepared.preserve()
                return .publishedButUnsynced(prepared.config,
                    "VM 파일과 등록 정보는 보존했지만 저장 완료를 확인하지 못했습니다. 라이브러리를 다시 확인하세요: \(error.localizedDescription)")
            }
        } catch {
            return .failed("VM 번들을 만들지 못했습니다: \(error.localizedDescription)")
        }
    }
}
