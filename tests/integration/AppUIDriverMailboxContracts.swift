import Darwin
import Foundation

enum AppUIDriverMailboxContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        let root = try T.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root.deletingLastPathComponent()) }
        try T.refuses("mailbox requires prepared children") { _ = try AppUIDriverMailbox(privateRoot: root) }
        try AppUIDriverMailbox.prepare(privateRoot: root)
        try T.refuses("mailbox preparation never replaces") { try AppUIDriverMailbox.prepare(privateRoot: root) }
        let box = try AppUIDriverMailbox(privateRoot: root)
        try T.check(try box.readSession() == nil, "absent immutable session")
        let session = T.session(root: root.path)
        let hash = try box.publishSession(session)
        try T.check(try box.readSession()?.session == session, "session roundtrip")
        try T.check(hash == T.hash(session), "session exact bytes hash")
        try T.refuses("session overwrite") { _ = try box.publishSession(session) }
        try T.refuses("session wrong parent") { _ = try box.publishSession(T.session()) }
        let request = try T.request(session: session)
        let requestHash = try box.publishRequest(request)
        try T.check(try box.readRequest(sequence: 1)?.request == request, "request roundtrip")
        try T.check(try box.readRequest(sequence: 1)?.sha256 == requestHash, "request hash bound")
        try T.refuses("request overwrite") { _ = try box.publishRequest(request) }
        try T.check(try box.readRequest(sequence: 1)?.request == request, "collision preserved original")
        let reply = try T.reply(request, values: .init(createVisible: true, importVisible: true), session: session)
        try box.publishReply(reply)
        try T.check(try box.readReply(sequence: 1) == reply, "reply roundtrip")
        let ready = try T.ready(session, trusted: false)
        try box.publishReady(ready)
        try T.check(try box.readReady() == ready, "actual trust result roundtrip")
        try T.refuses("ready overwrite") { try box.publishReady(ready) }
        try T.check(try box.readReply(sequence: 2) == nil, "absent reply is pending")
        try T.refuses("sequence0") { _ = try box.readRequest(sequence: 0) }
        try T.refuses("sequence1025") { _ = try box.readRequest(sequence: 1025) }
        try T.check(try !box.isCancelled(), "no cancellation")
        try T.rawFile(root.appendingPathComponent("cancel.requested"), data: Data())
        try T.check(try box.isCancelled(), "empty cancellation marker accepted")
        try malformedFiles(root: root, box: box, session: session)
        try directories(root)
        try completions(root: root, box: box, session: session)
    }

    private static func malformedFiles(root: URL, box: AppUIDriverMailbox, session: AppUIDriverSession) throws {
        typealias T = AppUIDriverProtocolTestSupport
        let requests = root.appendingPathComponent("driver-requests")
        let target = root.appendingPathComponent("outside-request.json")
        try T.rawFile(target, data: AppUIDriverCanonicalJSON.encode(T.request(sequence: 2, session: session)))
        let linked = requests.appendingPathComponent("000002.json")
        try FileManager.default.createSymbolicLink(at: linked, withDestinationURL: target)
        try T.refuses("request symlink") { _ = try box.readRequest(sequence: 2) }
        try T.check(link(target.path, requests.appendingPathComponent("000003.json").path) == 0, "hardlink fixture")
        try T.refuses("request hardlink") { _ = try box.readRequest(sequence: 3) }
        try T.check(mkfifo(requests.appendingPathComponent("000004.json").path, 0o600) == 0, "FIFO fixture")
        try T.refuses("FIFO does not block regular file check") { _ = try box.readRequest(sequence: 4) }
        let wrongMode = requests.appendingPathComponent("000005.json")
        try T.rawFile(wrongMode, data: AppUIDriverCanonicalJSON.encode(T.request(sequence: 5, session: session)))
        try T.check(chmod(wrongMode.path, 0o644) == 0, "mode fixture")
        try T.refuses("world-readable request") { _ = try box.readRequest(sequence: 5) }
        try T.rawFile(requests.appendingPathComponent("000006.json"), data: Data(repeating: 0, count: 8193))
        try T.refuses("oversized regular request") { _ = try box.readRequest(sequence: 6) }
        try T.rawFile(requests.appendingPathComponent("000007.json"), data: Data("{".utf8))
        try T.refuses("partial published JSON") { _ = try box.readRequest(sequence: 7) }
        let staging = requests.appendingPathComponent(".000008.json.staging")
        try T.rawFile(staging, data: AppUIDriverCanonicalJSON.encode(T.request(sequence: 8, session: session)))
        try T.check(try box.readRequest(sequence: 8) == nil, "unpublished staging invisible")
        try T.refuses("stale staging never overwritten") { _ = try box.publishRequest(T.request(sequence: 8, session: session)) }
        try FileManager.default.removeItem(at: staging)
        _ = try box.publishRequest(T.request(sequence: 8, session: session))
        try T.check(try box.readRequest(sequence: 8)?.request.sequence == 8, "fresh atomic publish")
    }

    private static func directories(_ root: URL) throws {
        typealias T = AppUIDriverProtocolTestSupport
        try T.check(chmod(root.path, 0o755) == 0, "directory mode fixture")
        try T.refuses("private root must be0700") { _ = try AppUIDriverMailbox(privateRoot: root) }
        try T.check(chmod(root.path, 0o700) == 0, "restore directory mode")
        let alias = root.deletingLastPathComponent().appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: root.deletingLastPathComponent())
        try T.refuses("symlink ancestor") { _ = try AppUIDriverMailbox(privateRoot: alias.appendingPathComponent("app-ui-private")) }
        let requests = root.appendingPathComponent("driver-requests"), moved = root.appendingPathComponent("held-requests")
        try FileManager.default.moveItem(at: requests, to: moved)
        try FileManager.default.createSymbolicLink(at: requests, withDestinationURL: moved)
        try T.refuses("request directory symlink") { _ = try AppUIDriverMailbox(privateRoot: root) }
        try FileManager.default.removeItem(at: requests)
        try FileManager.default.moveItem(at: moved, to: requests)
    }

    private static func completions(root: URL, box: AppUIDriverMailbox, session: AppUIDriverSession) throws {
        typealias T = AppUIDriverProtocolTestSupport
        let completion = AppUIDriverCompletion(sessionSHA256: try T.hash(session), nonce: session.nonce,
            hostPID: session.host.pid, driverPID: session.driver.pid, success: false, failureCode: nil,
            requestsProcessed: 12, mutationsPerformed: 6)
        try box.publishCompletion(completion)
        try T.check(try box.readCompletion() == completion, "host failure remains driver false without invented AX error")
        let invalid = AppUIDriverCompletion(sessionSHA256: try T.hash(session), nonce: session.nonce,
            hostPID: session.host.pid, driverPID: session.driver.pid, success: true, failureCode: nil,
            requestsProcessed: 11, mutationsPerformed: 5)
        try T.refuses("completion cannot promote missing actions") { try box.publishCompletion(invalid) }
        let observations = root.appendingPathComponent("host-observations")
        try FileManager.default.createDirectory(at: observations, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        try T.check(try box.readHostCompletion() == nil, "host completion pending")
        var legacy: [String: Any] = ["schema_version": 1, "kind": "native-app-ui-host-completion",
            "pid": Int(session.host.pid), "success": true, "cleanup_verified": true,
            "report_sha256": String(repeating: "a", count: 64), "failure": NSNull()]
        let data = try JSONSerialization.data(withJSONObject: legacy, options: [.prettyPrinted, .sortedKeys])
        try T.rawFile(observations.appendingPathComponent("host-completion.json"), data: data)
        try T.check(try box.readHostCompletion()?.success == true, "legacy exact Boolean success")
        legacy["success"] = 1
        try T.refuses("legacy number is not Boolean") {
            _ = try AppUIDriverHostCompletion.decode(JSONSerialization.data(withJSONObject: legacy, options: [.prettyPrinted, .sortedKeys]))
        }
        let text = String(decoding: data, as: UTF8.self)
        let duplicate = Data(text.replacingOccurrences(of: "\"success\" : true", with: "\"success\" : true,\n  \"success\" : true").utf8)
        try T.check(duplicate != data, "legacy duplicate counterexample differs")
        try T.refuses("legacy duplicate key") { _ = try AppUIDriverHostCompletion.decode(duplicate) }
    }
}
