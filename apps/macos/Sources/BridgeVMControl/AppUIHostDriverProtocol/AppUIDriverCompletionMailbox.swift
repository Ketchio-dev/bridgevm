#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

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
