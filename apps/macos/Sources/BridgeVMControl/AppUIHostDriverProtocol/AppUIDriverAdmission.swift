#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIDriverAdmission {
    let session: AppUIDriverSession
    let sessionSHA256: String
    private(set) var nextSequence: UInt32 = 1
    private(set) var phase: AppUIDriverPhase?
    private(set) var phaseDeadline: Double?
    private(set) var pending: AppUIDriverRequest?
    private(set) var terminal = false
    private var phaseComplete = false
    private var mutationAdmitted = false

    init(session: AppUIDriverSession, sessionSHA256: String, now: Double) throws {
        try AppUIDriverValidation.session(session)
        guard AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(session)) == sessionSHA256,
              now.isFinite, now >= session.startedUptime, now < session.deadlineUptime else {
            throw AppUIDriverFailure.invalidSession
        }
        self.session = session; self.sessionSHA256 = sessionSHA256
    }

    mutating func admit(_ request: AppUIDriverRequest, now: Double) throws {
        var candidate = self
        try candidate.admitValidated(request, now: now)
        self = candidate
    }

    private mutating func admitValidated(_ request: AppUIDriverRequest, now: Double) throws {
        guard !terminal, pending == nil, request.sequence == nextSequence else { throw AppUIDriverFailure.outOfOrder }
        try AppUIDriverValidation.request(request, session: session, sessionSHA256: sessionSHA256, now: now)
        if request.phase != phase {
            let phases = AppUIDriverPhase.allCases
            let expected: Int
            if let phase {
                guard let previous = phases.firstIndex(of: phase) else { throw AppUIDriverFailure.outOfOrder }
                expected = previous + 1
            } else { expected = 0 }
            guard expected < phases.count, request.phase == phases[expected],
                  phase == nil || phaseComplete else { throw AppUIDriverFailure.outOfOrder }
            phase = request.phase; phaseDeadline = request.phaseDeadlineUptime
            phaseComplete = false; mutationAdmitted = false
        } else if request.phaseDeadlineUptime != phaseDeadline {
            throw AppUIDriverFailure.deadlineExceeded
        }
        if AppUIDriverOperationPolicy.isMutation(request.operation) {
            guard !mutationAdmitted, request.operation == AppUIDriverOperationPolicy.mutation(for: request.phase) else {
                throw AppUIDriverFailure.replayedMutation
            }
            mutationAdmitted = true
        } else if AppUIDriverOperationPolicy.mutation(for: request.phase) != nil && !mutationAdmitted {
            throw AppUIDriverFailure.outOfOrder
        }
        pending = request
    }

    mutating func complete(_ reply: AppUIDriverReply, now: Double) throws {
        guard !terminal, let request = pending else { throw AppUIDriverFailure.outOfOrder }
        guard now.isFinite, now >= session.startedUptime, now < session.deadlineUptime,
              now < request.phaseDeadlineUptime else {
            terminal = true
            throw AppUIDriverFailure.deadlineExceeded
        }
        try AppUIDriverValidation.reply(reply, request: request, session: session,
            requestSHA256: AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(request)))
        pending = nil; nextSequence += 1
        if reply.outcome == .refused { terminal = true; return }
        phaseComplete = AppUIDriverOperationPolicy.completes(request.phase, reply: reply)
        if phaseComplete && request.phase == .clearing { terminal = true }
    }
}
#endif
