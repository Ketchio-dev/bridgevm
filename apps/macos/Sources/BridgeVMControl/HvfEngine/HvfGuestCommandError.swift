import Foundation

enum HvfGuestCommandError: Error, Equatable {
    case empty
    case multiline
    case tooLong(actual: Int, maximum: Int)
    case protocolVerb(String)

    var message: String {
        switch self {
        case .empty:
            return "게스트 명령이 비어 있습니다."
        case .multiline:
            return "게스트 명령에는 개행 또는 NUL 문자를 사용할 수 없습니다."
        case let .tooLong(actual, maximum):
            return "게스트 명령이 너무 깁니다(\(actual)/\(maximum) bytes)."
        case let .protocolVerb(verb):
            return "대문자 '\(verb)'(으)로 시작하는 명령은 게스트 에이전트 프로토콜 명령으로 해석되어 셸에서 실행되지 않습니다. "
                + "Windows 명령은 대소문자를 구분하지 않으니 소문자 '\(verb.lowercased())'(으)로 입력해 주세요."
        }
    }
}
