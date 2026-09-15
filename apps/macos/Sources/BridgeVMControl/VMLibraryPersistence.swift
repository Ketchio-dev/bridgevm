import Foundation

extension VMLibrary {
    @discardableResult
    static func save(_ config: VMConfig, rootURL: URL = root) -> Bool {
        saveOutcome(config, rootURL: rootURL).isCommitted
    }

    static func saveOutcome(
        _ config: VMConfig, rootURL: URL = root,
        publish: (Data, URL) -> VMRegistrationCommitOutcome = { VMRegistrationWriter.commit($0, to: $1) }
    ) -> VMRegistrationCommitOutcome {
        var cfg = config
        cfg.id = cfg.slug
        let dir = rootURL.appendingPathComponent(cfg.slug, isDirectory: true)
        let configURL = dir.appendingPathComponent("vm.json")
        do {
            if (try? dir.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true
                || (try? configURL.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
                return .notPublished(CocoaError(.fileWriteInvalidFileName))
            }
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(cfg)
            guard data.count <= maximumConfigBytes else {
                return .notPublished(CocoaError(.fileWriteUnknown,
                    userInfo: [NSLocalizedDescriptionKey: "VM 등록 정보가 최대 크기(\(maximumConfigBytes)바이트)를 초과했습니다."]))
            }
            return publish(data, configURL)
        } catch {
            return .notPublished(error)
        }
    }
}
