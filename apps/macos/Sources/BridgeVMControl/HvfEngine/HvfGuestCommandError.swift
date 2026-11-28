import Foundation

enum HvfGuestCommandError: Error, Equatable {
    case empty
    case multiline
    case tooLong(actual: Int, maximum: Int)
    case protocolVerb(String)

    var message: String {
        switch self {
        case .empty:
            return "Enter a Windows command."
        case .multiline:
            return "Enter the command on one line without null characters."
        case let .tooLong(actual, maximum):
            return "The command uses \(actual) bytes and the limit is \(maximum) bytes."
        case let .protocolVerb(verb):
            return "Guest Tools reserves the uppercase command “\(verb)”. "
                + "Enter lowercase “\(verb.lowercased())” to run it in the Windows shell."
        }
    }
}
