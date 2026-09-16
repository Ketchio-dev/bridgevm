import Foundation

struct NativeRuntimeRequestContext: Sendable {
    let deadline: TimeInterval

    func validateAdmission() throws {
        try Task.checkCancellation()
        try NativeRuntimeTransport.checkDeadline(deadline)
    }
}
