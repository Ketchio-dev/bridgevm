import Foundation

enum NativeCLIError: LocalizedError {
    case invalid(String), unavailable(String)
    var errorDescription: String? {
        switch self { case .invalid(let text), .unavailable(let text): return text }
    }
}
