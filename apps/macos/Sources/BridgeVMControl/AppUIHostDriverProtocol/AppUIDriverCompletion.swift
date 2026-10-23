#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIDriverCompletion: Codable, Equatable {
    /// Each restart follows a yield of at least 25 ms, so the 90 s session admits at most 3,600.
    static let maximumAXRestarts = 3_600
    var schemaVersion = 1
    var kind = "native-app-ui-driver-completion"
    let sessionSHA256: String
    let nonce: String
    let hostPID: Int32
    let driverPID: Int32
    let success: Bool
    let failureCode: AppUIDriverFailure?
    let requestsProcessed: Int
    let mutationsPerformed: Int
    /// Read-only AX passes restarted after a transient AX status, over the whole session.
    let axRestarts: Int
}

extension AppUIDriverValidation {
    static func completion(_ value: AppUIDriverCompletion, session: AppUIDriverSession,
                           sessionSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-driver-completion",
              value.sessionSHA256 == sessionSHA256, hexDigest(sessionSHA256), value.nonce == session.nonce,
              value.hostPID == session.host.pid, value.driverPID == session.driver.pid,
              (0...Int(AppUIDriverConstants.maximumSequence)).contains(value.requestsProcessed),
              (0...6).contains(value.mutationsPerformed), value.mutationsPerformed <= value.requestsProcessed,
              (0...AppUIDriverCompletion.maximumAXRestarts).contains(value.axRestarts),
              !value.success || (value.failureCode == nil && value.mutationsPerformed == 6 && value.requestsProcessed >= 12)
        else { throw AppUIDriverFailure.invalidReply }
    }
}
#endif
