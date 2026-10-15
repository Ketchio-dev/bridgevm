#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIInstallCancelHostCompletion: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-install-cancel-host-completion"
    let pid: Int32
    let success: Bool
    let settled: Bool
    let fixturePreserved: Bool
    let reportSHA256: String
    let failureCode: AppUIDriverFailure?
}

extension AppUIInstallCancelValidation {
    static func hostCompletion(_ value: AppUIInstallCancelHostCompletion, session: AppUIInstallCancelSession) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-install-cancel-host-completion",
              value.pid == session.host.pid, AppUIDriverValidation.hexDigest(value.reportSHA256),
              !value.success || (value.settled && value.fixturePreserved && value.failureCode == nil) else {
            throw AppUIDriverFailure.invalidReply
        }
    }
}

extension AppUIInstallCancelMailbox {
    func publishHostCompletion(_ completion: AppUIInstallCancelHostCompletion) throws {
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        try AppUIInstallCancelValidation.hostCompletion(completion, session: session.session)
        let observations = try root.child("host-observations")
        try observations.publish(AppUIDriverCanonicalJSON.encode(completion), name: "install-cancel-host-completion.json")
    }

    func readHostCompletion() throws -> AppUIInstallCancelHostCompletion? {
        let observations = try root.child("host-observations")
        guard let data = try observations.read("install-cancel-host-completion.json") else { return nil }
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        let completion = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelHostCompletion.self)
        try AppUIInstallCancelValidation.hostCompletion(completion, session: session.session)
        return completion
    }
}
#endif
