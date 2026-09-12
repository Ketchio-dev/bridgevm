import Foundation

extension VMRelocationJournal {
    static func recoveryIssue(_ config: VMConfig, rootURL: URL) -> String {
        let fallback = "VM 이동 복구 기록이 남아 있어 불러오지 않았습니다. 원본과 대상 번들 및 등록 상태를 확인하세요."
        let recordURL = url(config, rootURL: rootURL)
        guard let data = VMRelocationRecordReader.read(recordURL),
              let record = try? JSONDecoder().decode(Record.self, from: data),
              record.schema == "bridgevm.relocation-pending.v1",
              record.original.slug == config.slug, record.destination.slug == config.slug,
              displayable(record.original.bundlePath), displayable(record.destination.bundlePath) else {
            return fallback
        }
        return fallback + "\n기록된 원본: " + record.original.bundlePath +
            "\n기록된 대상: " + record.destination.bundlePath +
            "\n이 경로 정보만으로 미디어의 완전성이나 복구 성공을 확인할 수는 없습니다."
    }

    private static func displayable(_ path: String) -> Bool {
        path.hasPrefix("/") && path.utf8.count <= 4096 &&
            path.rangeOfCharacter(from: .controlCharacters) == nil
    }
}
