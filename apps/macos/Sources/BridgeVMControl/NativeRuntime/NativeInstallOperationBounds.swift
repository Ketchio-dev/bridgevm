import Foundation

extension NativeInstallOperation {
    func boundedLogs(_ input: [String]) -> [String] {
        var result: [String] = [], bytes = 0
        for line in input.suffix(NativeInstallControlCodec.maximumLogLines).reversed() {
            let remaining = NativeInstallControlCodec.maximumLogBytes - bytes
            guard remaining > 0 else { break }
            let value = bounded(line, limit: min(remaining, NativeInstallControlCodec.maximumLogLineBytes))
            guard !value.isEmpty else { break }
            result.append(value); bytes += value.utf8.count
        }
        return result.reversed()
    }

    func bounded(_ value: String, limit: Int) -> String {
        guard value.utf8.count > limit else { return value }
        let prefix = Array(value.utf8.prefix(limit))
        for count in stride(from: prefix.count, through: 0, by: -1) {
            if let result = String(bytes: prefix.prefix(count), encoding: .utf8) { return result }
        }
        return ""
    }
}
