import Foundation

enum HVFCreationOutcome: Sendable {
    case created(VMConfig)
    case publishedButUnsynced(VMConfig, String)
}
