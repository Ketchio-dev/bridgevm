import Foundation

enum HVFCreationOutcome: Sendable {
    case created(VMConfig)
    case publishedButUnsynced(VMConfig, String)
    /// Nothing was published; the sentence tells the user why and what to do.
    case refused(String)
}
