import Foundation

enum HvfGuestCommandError: Error, Equatable {
    case empty
    case multiline
    case tooLong(actual: Int, maximum: Int)

    var message: String {
        switch self {
        case .empty:
            return "게스트 명령이 비어 있습니다."
        case .multiline:
            return "게스트 명령에는 개행 또는 NUL 문자를 사용할 수 없습니다."
        case let .tooLong(actual, maximum):
            return "게스트 명령이 너무 깁니다(\(actual)/\(maximum) bytes)."
        }
    }
}

enum HvfGuestCommand {
    static let maximumBytes = 64 * 1024

    static func normalize(_ value: String) -> Result<String, HvfGuestCommandError> {
        let command = value.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty else { return .failure(.empty) }
        guard !command.contains("\n"), !command.contains("\r"), !command.contains("\0") else {
            return .failure(.multiline)
        }
        let byteCount = command.lengthOfBytes(using: .utf8)
        guard byteCount <= maximumBytes else {
            return .failure(.tooLong(actual: byteCount, maximum: maximumBytes))
        }
        return .success(command)
    }
}
