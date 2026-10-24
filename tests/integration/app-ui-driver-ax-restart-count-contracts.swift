import Foundation

/// Where the restart count is recorded: the driver completion's `axRestarts`, an
/// exact bounded integer its validator requires on publication and on every read.
extension AppUIDriverAXRestartContracts {
    typealias T = AppUIDriverProtocolTestSupport

    static func completion(_ restarts: Int, success: Bool = true, session: AppUIDriverSession,
                           hash: String) -> AppUIDriverCompletion {
        AppUIDriverCompletion(sessionSHA256: hash, nonce: session.nonce, hostPID: session.host.pid,
            driverPID: session.driver.pid, success: success, failureCode: nil, requestsProcessed: 12,
            mutationsPerformed: 6, axRestarts: restarts)
    }

    static func restartCountContracts() {
        let session = T.session()
        guard let hash = try? T.hash(session) else { return check(false, "the fixture session hashes") }
        func valid(_ restarts: Int, success: Bool) -> Bool {
            let value = completion(restarts, success: success, session: session, hash: hash)
            return (try? AppUIDriverValidation.completion(value, session: session, sessionSHA256: hash)) != nil
        }
        for restarts in [0, 1, 199, AppUIDriverCompletion.maximumAXRestarts] {
            check(valid(restarts, success: true) && valid(restarts, success: false), "a completion records \(restarts) restarts")
        }
        for restarts in [-1, AppUIDriverCompletion.maximumAXRestarts + 1, Int.max] {
            check(!valid(restarts, success: true) && !valid(restarts, success: false), "a completion refuses \(restarts) restarts")
        }
        check(AppUIDriverCompletion.maximumAXRestarts == Int((90 / AppUIDriverAXRestart.pause).rounded()),
              "the bound is one paced restart per 25 ms of the 90 s session")

        guard let data = try? AppUIDriverCanonicalJSON.encode(completion(2, session: session, hash: hash)),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return check(false, "a completion encodes")
        }
        func decodes(_ change: (inout [String: Any]) -> Void) -> Bool {
            var value = object
            change(&value)
            guard let bytes = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
            else { return false }
            return (try? AppUIDriverCanonicalJSON.decode(bytes, as: AppUIDriverCompletion.self)) != nil
        }
        check(object["axRestarts"] as? Int == 2 && decodes { _ in }, "the count is encoded exactly as axRestarts")
        check(!decodes { $0.removeValue(forKey: "axRestarts") }, "a completion without axRestarts is refused")
        check(!decodes { $0["axRestarts"] = true } && !decodes { $0["axRestarts"] = "2" } && !decodes { $0["axRestarts"] = 2.5 },
              "a count that is not an integer is refused")
        check(!decodes { $0["restarts"] = 2 }, "an unknown key beside the count is refused")
        mailboxRecordsTheCount()
        installCompletionKeepsItsOwnKind(session: session, d6: data)
    }

    /// The supervisor's actual read path validates the count; a refused one is never published.
    static func mailboxRecordsTheCount() {
        do {
            let root = try T.temporaryRoot()
            defer { try? FileManager.default.removeItem(at: root.deletingLastPathComponent()) }
            try AppUIDriverMailbox.prepare(privateRoot: root)
            let box = try AppUIDriverMailbox(privateRoot: root)
            let session = T.session(root: root.path)
            let hash = try box.publishSession(session)
            let unbounded = completion(AppUIDriverCompletion.maximumAXRestarts + 1, session: session, hash: hash)
            check((try? box.publishCompletion(unbounded)) == nil, "publication refuses an unbounded count")
            check(try box.readCompletion() == nil, "a refused completion is never published")
            let recorded = completion(3, success: false, session: session, hash: hash)
            try box.publishCompletion(recorded)
            check(try box.readCompletion() == recorded, "the supervisor reads the recorded count back exactly")
        } catch { check(false, "the completion mailbox fixture: \(error)") }
    }

    /// Before the count, a d6 completion decoded as an install completion and only the kind refused
    /// it; that kind check is still exercised here, and the d6 bytes no longer decode at all.
    static func installCompletionKeepsItsOwnKind(session: AppUIDriverSession, d6: Data) {
        let install = AppUIInstallCancelSession(nonce: session.nonce, startedUptime: session.startedUptime,
            deadlineUptime: session.deadlineUptime, host: session.host, driver: session.driver)
        guard let hash = try? AppUIDriverCanonicalJSON.digest(AppUIDriverCanonicalJSON.encode(install)) else {
            return check(false, "the install session hashes")
        }
        var completion = AppUIInstallCancelCompletion(sessionSHA256: hash, nonce: install.nonce, hostPID: install.host.pid,
            driverPID: install.driver.pid, success: true, failureCode: nil, requestsProcessed: 8, mutationsPerformed: 3)
        func valid() -> Bool {
            (try? AppUIInstallCancelValidation.completion(completion, session: install, sessionSHA256: hash)) != nil
        }
        check(valid(), "an install completion with its own kind is accepted")
        completion.kind = "native-app-ui-driver-completion"
        check(!valid(), "the install validator still refuses the d6 completion kind")
        check((try? AppUIDriverCanonicalJSON.decode(d6, as: AppUIInstallCancelCompletion.self)) == nil,
              "a d6 completion with its restart count never decodes as an install completion")
    }
}
