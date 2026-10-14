#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIInstallCancelCompletion: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-install-cancel-completion"
    let sessionSHA256: String
    let nonce: String
    let hostPID: Int32
    let driverPID: Int32
    let success: Bool
    let failureCode: AppUIDriverFailure?
    let requestsProcessed: Int
    let mutationsPerformed: Int
}

extension AppUIInstallCancelValidation {
    static func completion(_ value: AppUIInstallCancelCompletion, session: AppUIInstallCancelSession,
                           sessionSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-install-cancel-completion",
              value.sessionSHA256 == sessionSHA256, AppUIDriverValidation.hexDigest(sessionSHA256),
              value.nonce == session.nonce, value.hostPID == session.host.pid, value.driverPID == session.driver.pid,
              (0...Int(AppUIDriverConstants.maximumSequence)).contains(value.requestsProcessed),
              (0...3).contains(value.mutationsPerformed), value.mutationsPerformed <= value.requestsProcessed,
              !value.success || (value.failureCode == nil && value.mutationsPerformed == 3 && value.requestsProcessed >= 8)
        else { throw AppUIDriverFailure.invalidReply }
    }
}

extension AppUIInstallCancelMailbox {
    func publishCompletion(_ completion: AppUIInstallCancelCompletion) throws {
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        try AppUIInstallCancelValidation.completion(completion, session: session.session, sessionSHA256: session.sha256)
        try root.publish(AppUIDriverCanonicalJSON.encode(completion), name: "install-cancel-completion.json")
    }

    func readCompletion() throws -> AppUIInstallCancelCompletion? {
        guard let data = try root.read("install-cancel-completion.json") else { return nil }
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        let completion = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelCompletion.self)
        try AppUIInstallCancelValidation.completion(completion, session: session.session, sessionSHA256: session.sha256)
        return completion
    }
}
#endif
