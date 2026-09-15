import Foundation

extension ControlModel {
    static func packageInstallCommand(_ packages: [String]) -> String? {
        let names = packages.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        guard !names.isEmpty,
              names.allSatisfy({
                  !$0.isEmpty && $0.range(
                      of: #"^[a-z0-9][a-z0-9+.-]*(?::[a-z0-9][a-z0-9-]*)?$"#,
                      options: .regularExpression
                  ) != nil
              }) else { return nil }
        let arguments = names.map(Shell.shQuote).joined(separator: " ")
        return "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \(arguments) 2>&1 | tail -25"
    }

    static func boundedLog(_ value: String, limit: Int) -> String {
        guard limit > 0, value.count > limit else { return value }
        return "… 이전 로그 생략 …\n" + String(value.suffix(limit))
    }
}
