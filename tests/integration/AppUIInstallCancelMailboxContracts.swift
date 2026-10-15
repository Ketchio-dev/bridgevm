import Darwin
import Foundation

enum AppUIInstallCancelMailboxContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        typealias F = AppUIInstallCancelProtocolTestSupport
        let root = try T.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root.deletingLastPathComponent()) }
        try T.refuses("install mailbox needs prepared children") { _ = try AppUIInstallCancelMailbox(privateRoot: root) }
        try AppUIInstallCancelMailbox.prepare(privateRoot: root)
        try T.refuses("install mailbox preparation cannot replace") { try AppUIInstallCancelMailbox.prepare(privateRoot: root) }
        let box = try AppUIInstallCancelMailbox(privateRoot: root), session = F.session(root: root.path)
        try T.check(try box.readSession() == nil, "install absent session")
        let hash = try box.publishSession(session)
        try T.check(try box.readSession()?.session == session && hash == F.hash(session), "install session hash roundtrip")
        try T.refuses("install session cannot overwrite") { _ = try box.publishSession(session) }
        try T.refuses("install session parent identity") { _ = try box.publishSession(F.session()) }
        let legacy = try AppUIDriverMailbox(privateRoot: root)
        try T.check(try legacy.readSession() == nil, "install session is invisible to v2 path")
        _ = try legacy.publishSession(T.session(root: root.path))
        try T.check(try box.readSession()?.session == session, "legacy session cannot replace install session")
        let request = try F.request(session: session), requestHash = try box.publishRequest(request)
        try T.check(try box.readRequest(sequence: 1)?.request == request
            && box.readRequest(sequence: 1)?.sha256 == requestHash, "install request hash roundtrip")
        try T.refuses("install request cannot overwrite") { _ = try box.publishRequest(request) }
        try T.refuses("v2 rejects install request") { _ = try legacy.readRequest(sequence: 1) }
        let reply = try F.reply(request, values: F.values(.idle), session: session)
        try box.publishReply(reply)
        try T.check(try box.readReply(sequence: 1) == reply, "install reply roundtrip")
        try T.check(try box.readReply(sequence: 2) == nil, "install absent reply is pending")
        try T.refuses("install sequence zero") { _ = try box.readRequest(sequence: 0) }
        try T.refuses("install sequence1025") { _ = try box.readRequest(sequence: 1025) }
        let ready = try F.ready(session, trusted: false)
        try box.publishReady(ready)
        try T.check(try box.readReady() == ready && legacy.readReady() == nil, "install ready path isolated")
        let completion = try F.completion(session)
        try box.publishCompletion(completion)
        try T.check(try box.readCompletion() == completion && legacy.readCompletion() == nil, "install completion path isolated")
        try T.refuses("install completion immutable") { try box.publishCompletion(completion) }
        try host(root: root, box: box, session: session)
        try T.check(try !box.isCancelled(), "install initially uncancelled")
        try T.rawFile(root.appendingPathComponent("cancel.requested"), data: Data())
        try T.check(try box.isCancelled(), "install own shared cancellation marker")
        var admission = try AppUIInstallCancelAdmission(session: session, sessionSHA256: hash, now: 100)
        try admission.admit(request, now: 100)
        if try box.isCancelled() { admission.abort(.cancelled) }
        try T.check(admission.terminal && admission.failureCode == .cancelled, "install file cancellation aborts admission")
        try contamination(root: root, box: box)
    }

    private static func host(root: URL, box: AppUIInstallCancelMailbox, session: AppUIInstallCancelSession) throws {
        typealias T = AppUIDriverProtocolTestSupport
        let observations = root.appendingPathComponent("host-observations")
        try FileManager.default.createDirectory(at: observations, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
        try T.check(try box.readHostCompletion() == nil, "install host completion pending")
        let completion = AppUIInstallCancelHostCompletion(pid: session.host.pid, success: true, settled: true,
            fixturePreserved: true, reportSHA256: String(repeating: "d", count: 64), failureCode: nil)
        try box.publishHostCompletion(completion)
        try T.check(try box.readHostCompletion() == completion, "install host completion roundtrip")
        try T.refuses("install host completion immutable") { try box.publishHostCompletion(completion) }
        try T.check(try AppUIDriverMailbox(privateRoot: root).readHostCompletion() == nil,
            "install host completion cannot satisfy v2 filename")
    }

    private static func contamination(root: URL, box: AppUIInstallCancelMailbox) throws {
        typealias T = AppUIDriverProtocolTestSupport
        let requestDirectory = root.appendingPathComponent("driver-requests")
        try T.rawFile(requestDirectory.appendingPathComponent("000002.json"),
            data: AppUIDriverCanonicalJSON.encode(T.request(sequence: 2)))
        try T.refuses("install reader rejects v2 request") { _ = try box.readRequest(sequence: 2) }
        let target = root.appendingPathComponent("unowned-request.json")
        try T.rawFile(target, data: AppUIDriverCanonicalJSON.encode(AppUIInstallCancelProtocolTestSupport.request(sequence: 3)))
        try FileManager.default.createSymbolicLink(at: requestDirectory.appendingPathComponent("000003.json"), withDestinationURL: target)
        try T.refuses("install reader retains symlink refusal") { _ = try box.readRequest(sequence: 3) }
        try T.rawFile(requestDirectory.appendingPathComponent("000004.json"), data: Data(repeating: 32, count: 8193))
        try T.refuses("install reader retains8KiB bound") { _ = try box.readRequest(sequence: 4) }
        try T.check(chmod(root.path, 0o755) == 0, "install root permission fixture")
        try T.refuses("install root must remain private") { _ = try AppUIInstallCancelMailbox(privateRoot: root) }
        try T.check(chmod(root.path, 0o700) == 0, "install root permission restored")
    }
}
