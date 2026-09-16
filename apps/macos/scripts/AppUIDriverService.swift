import AppKit
import Foundation

@MainActor
final class AppUIDriverService {
    let mailbox: AppUIDriverMailbox
    let session: AppUIDriverSession
    let sessionSHA256: String
    private var admission: AppUIDriverAdmission
    private var requestsProcessed = 0
    private var mutationsPerformed = 0
    private let hostApplication: NSRunningApplication
    private var now: Double { ProcessInfo.processInfo.systemUptime }

    init(mailbox: AppUIDriverMailbox, session: AppUIDriverSession, sha256: String) throws {
        self.mailbox = mailbox; self.session = session; sessionSHA256 = sha256
        hostApplication = try AppUIDriverIdentity.verify(session.host, privateRoot: mailbox.privateRoot, driver: false)
        admission = try AppUIDriverAdmission(session: session, sessionSHA256: sha256,
                                            now: ProcessInfo.processInfo.systemUptime)
    }

    func run() -> Int32 {
        do {
            while true {
                guard try !mailbox.isCancelled() else { throw AppUIDriverFailure.cancelled }
                guard now < session.deadlineUptime else { throw AppUIDriverFailure.deadlineExceeded }
                if hostApplication.isTerminated {
                    guard let host = try mailbox.readHostCompletion() else { throw AppUIDriverFailure.invalidFile }
                    guard host.pid == session.host.pid else { throw AppUIDriverFailure.identityMismatch }
                    if host.success {
                        guard admission.terminal, admission.phase == .clearing,
                              requestsProcessed >= 12, mutationsPerformed == 6 else {
                            throw AppUIDriverFailure.outOfOrder
                        }
                    }
                    return finish(success: host.success, failure: nil)
                }
                if !admission.terminal,
                   let item = try mailbox.readRequest(sequence: admission.nextSequence) {
                    try serve(item.request, sha256: item.sha256)
                }
                _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
            }
        } catch {
            return finish(success: false, failure: Self.failure(error).code)
        }
    }

    private func serve(_ request: AppUIDriverRequest, sha256: String) throws {
        requestsProcessed += 1
        do {
            try admission.admit(request, now: now)
            try AppUIDriverIdentity.verify(session.driver, privateRoot: mailbox.privateRoot, driver: true)
            try AppUIDriverIdentity.verify(session.host, privateRoot: mailbox.privateRoot, driver: false)
            let access = AppUIDriverAX(pid: session.host.pid,
                deadline: min(session.deadlineUptime, request.phaseDeadlineUptime),
                cancelled: { try self.mailbox.isCancelled() })
            let result = try AppUIDriverAXOperations(access: access).perform(request.operation)
            if result.outcome == .performed { mutationsPerformed += 1 }
            let reply = reply(request, sha256: sha256, outcome: result.outcome,
                              values: result.values, failure: nil, axError: nil)
            try admission.complete(reply, now: now)
            try mailbox.publishReply(reply)
        } catch {
            let failure = Self.failure(error)
            let refusal = reply(request, sha256: sha256, outcome: .refused, values: nil,
                                failure: failure.code, axError: failure.axError)
            // A refused or uncertain mutation is terminal and is never replayed.
            try mailbox.publishReply(refusal)
            throw error
        }
    }

    private func reply(_ request: AppUIDriverRequest, sha256: String, outcome: AppUIDriverOutcome,
                       values: AppUIDriverValues?, failure: AppUIDriverFailure?, axError: Int32?) -> AppUIDriverReply {
        AppUIDriverReply(sessionSHA256: sessionSHA256, nonce: session.nonce,
            sequence: request.sequence, requestSHA256: sha256, hostPID: session.host.pid,
            driverPID: session.driver.pid, operation: request.operation, outcome: outcome,
            values: values, failureCode: failure, axError: axError)
    }

    func finish(success: Bool, failure: AppUIDriverFailure?) -> Int32 {
        do {
            try mailbox.publishCompletion(AppUIDriverCompletion(sessionSHA256: sessionSHA256,
                nonce: session.nonce, hostPID: session.host.pid, driverPID: session.driver.pid,
                success: success, failureCode: failure, requestsProcessed: requestsProcessed,
                mutationsPerformed: mutationsPerformed))
        } catch {
            FileHandle.standardError.write(Data("app UI driver completion refused: \(Self.failure(error).code.rawValue)\n".utf8))
            return 2
        }
        return success ? 0 : 1
    }

    static func failure(_ error: Error) -> (code: AppUIDriverFailure, axError: Int32?) {
        if let error = error as? AppUIDriverAXError { return (error.failure, error.rawValue) }
        return (error as? AppUIDriverFailure ?? .internalFailure, nil)
    }
}
