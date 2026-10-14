#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIInstallCancelMailbox {
    let privateRoot: URL
    let root: AppUIDriverPrivateDirectory
    private let requests: AppUIDriverPrivateDirectory
    private let replies: AppUIDriverPrivateDirectory

    static func prepare(privateRoot: URL) throws {
        try AppUIDriverPrivateDirectory.root(privateRoot).createChildren()
    }

    init(privateRoot: URL) throws {
        self.privateRoot = privateRoot
        root = try AppUIDriverPrivateDirectory.root(privateRoot)
        requests = try root.child("driver-requests")
        replies = try root.child("driver-replies")
    }

    func publishSession(_ session: AppUIInstallCancelSession) throws -> String {
        try AppUIInstallCancelValidation.session(session)
        try validateRoot(session)
        return try publish(session, directory: root, name: "install-cancel-session.json")
    }

    func readSession() throws -> (session: AppUIInstallCancelSession, sha256: String)? {
        guard let data = try root.read("install-cancel-session.json") else { return nil }
        let session = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelSession.self)
        try AppUIInstallCancelValidation.session(session)
        try validateRoot(session)
        return (session, AppUIDriverCanonicalJSON.digest(data))
    }

    func publishRequest(_ request: AppUIInstallCancelRequest) throws -> String {
        guard request.schemaVersion == 1, request.kind == "native-app-ui-install-cancel-request" else {
            throw AppUIDriverFailure.invalidRequest
        }
        return try publish(request, directory: requests, name: name(request.sequence))
    }

    func readRequest(sequence: UInt32) throws -> (request: AppUIInstallCancelRequest, sha256: String)? {
        guard let data = try requests.read(name(sequence)) else { return nil }
        let request = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelRequest.self)
        guard request.sequence == sequence else { throw AppUIDriverFailure.outOfOrder }
        guard request.schemaVersion == 1, request.kind == "native-app-ui-install-cancel-request" else {
            throw AppUIDriverFailure.invalidRequest
        }
        return (request, AppUIDriverCanonicalJSON.digest(data))
    }

    func publishReply(_ reply: AppUIInstallCancelReply) throws {
        guard reply.schemaVersion == 1, reply.kind == "native-app-ui-install-cancel-reply" else {
            throw AppUIDriverFailure.invalidReply
        }
        _ = try publish(reply, directory: replies, name: name(reply.sequence))
    }

    func readReply(sequence: UInt32) throws -> AppUIInstallCancelReply? {
        guard let data = try replies.read(name(sequence)) else { return nil }
        let reply = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelReply.self)
        guard reply.sequence == sequence else { throw AppUIDriverFailure.outOfOrder }
        guard reply.schemaVersion == 1, reply.kind == "native-app-ui-install-cancel-reply" else {
            throw AppUIDriverFailure.invalidReply
        }
        return reply
    }

    func publishReady(_ ready: AppUIInstallCancelReady) throws {
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        try AppUIInstallCancelValidation.ready(ready, session: session.session, sessionSHA256: session.sha256)
        _ = try publish(ready, directory: root, name: "install-cancel-ready.json")
    }

    func readReady() throws -> AppUIInstallCancelReady? {
        guard let data = try root.read("install-cancel-ready.json") else { return nil }
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        let ready = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelReady.self)
        try AppUIInstallCancelValidation.ready(ready, session: session.session, sessionSHA256: session.sha256)
        return ready
    }

    func isCancelled() throws -> Bool { try root.cancellationExists() }

    private func validateRoot(_ session: AppUIInstallCancelSession) throws {
        guard URL(fileURLWithPath: session.host.bundlePath).deletingLastPathComponent().path == privateRoot.path else {
            throw AppUIDriverFailure.identityMismatch
        }
    }

    private func name(_ sequence: UInt32) throws -> String {
        guard (1...AppUIDriverConstants.maximumSequence).contains(sequence) else { throw AppUIDriverFailure.protocolLimit }
        return String(format: "%06u.json", sequence)
    }

    private func publish<T: Encodable>(_ value: T, directory: AppUIDriverPrivateDirectory, name: String) throws -> String {
        let data = try AppUIDriverCanonicalJSON.encode(value)
        try directory.publish(data, name: name)
        return AppUIDriverCanonicalJSON.digest(data)
    }
}
#endif
