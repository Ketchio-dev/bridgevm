#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIDriverMailbox {
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

    func publishSession(_ session: AppUIDriverSession) throws -> String {
        try AppUIDriverValidation.session(session)
        guard URL(fileURLWithPath: session.host.bundlePath).deletingLastPathComponent().path == privateRoot.path else {
            throw AppUIDriverFailure.identityMismatch
        }
        return try publish(session, directory: root, name: "driver-session.json")
    }

    func readSession() throws -> (session: AppUIDriverSession, sha256: String)? {
        guard let data = try root.read("driver-session.json") else { return nil }
        let session = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverSession.self)
        try AppUIDriverValidation.session(session)
        guard URL(fileURLWithPath: session.host.bundlePath).deletingLastPathComponent().path == privateRoot.path else {
            throw AppUIDriverFailure.identityMismatch
        }
        return (session, AppUIDriverCanonicalJSON.digest(data))
    }

    func publishRequest(_ request: AppUIDriverRequest) throws -> String {
        try publish(request, directory: requests, name: name(request.sequence))
    }

    func readRequest(sequence: UInt32) throws -> (request: AppUIDriverRequest, sha256: String)? {
        guard let data = try requests.read(name(sequence)) else { return nil }
        let request = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverRequest.self)
        guard request.sequence == sequence else { throw AppUIDriverFailure.outOfOrder }
        return (request, AppUIDriverCanonicalJSON.digest(data))
    }

    func publishReply(_ reply: AppUIDriverReply) throws {
        _ = try publish(reply, directory: replies, name: name(reply.sequence))
    }

    func readReply(sequence: UInt32) throws -> AppUIDriverReply? {
        guard let data = try replies.read(name(sequence)) else { return nil }
        let reply = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverReply.self)
        guard reply.sequence == sequence else { throw AppUIDriverFailure.outOfOrder }
        return reply
    }

    func publishReady(_ ready: AppUIDriverReady) throws {
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        try AppUIDriverValidation.ready(ready, session: session.session, sessionSHA256: session.sha256)
        _ = try publish(ready, directory: root, name: "driver-ready.json")
    }

    func readReady() throws -> AppUIDriverReady? {
        guard let data = try root.read("driver-ready.json") else { return nil }
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        let ready = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverReady.self)
        try AppUIDriverValidation.ready(ready, session: session.session, sessionSHA256: session.sha256)
        return ready
    }

    func isCancelled() throws -> Bool { try root.cancellationExists() }

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
