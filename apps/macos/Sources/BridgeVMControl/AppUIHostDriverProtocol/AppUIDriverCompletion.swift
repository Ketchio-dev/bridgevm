#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIDriverCompletion: Codable, Equatable {
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
}

extension AppUIDriverValidation {
    static func completion(_ value: AppUIDriverCompletion, session: AppUIDriverSession,
                           sessionSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-driver-completion",
              value.sessionSHA256 == sessionSHA256, hexDigest(sessionSHA256), value.nonce == session.nonce,
              value.hostPID == session.host.pid, value.driverPID == session.driver.pid,
              (0...Int(AppUIDriverConstants.maximumSequence)).contains(value.requestsProcessed),
              (0...6).contains(value.mutationsPerformed), value.mutationsPerformed <= value.requestsProcessed,
              !value.success || (value.failureCode == nil && value.mutationsPerformed == 6 && value.requestsProcessed >= 12)
        else { throw AppUIDriverFailure.invalidReply }
    }
}

extension AppUIDriverMailbox {
    func publishCompletion(_ completion: AppUIDriverCompletion) throws {
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        try AppUIDriverValidation.completion(completion, session: session.session, sessionSHA256: session.sha256)
        try root.publish(AppUIDriverCanonicalJSON.encode(completion), name: "driver-completion.json")
    }

    func readCompletion() throws -> AppUIDriverCompletion? {
        guard let data = try root.read("driver-completion.json") else { return nil }
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        let completion = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverCompletion.self)
        try AppUIDriverValidation.completion(completion, session: session.session, sessionSHA256: session.sha256)
        return completion
    }
}
#endif
