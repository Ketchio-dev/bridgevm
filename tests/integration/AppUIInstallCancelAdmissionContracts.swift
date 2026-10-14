import Foundation

enum AppUIInstallCancelAdmissionContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        typealias F = AppUIInstallCancelProtocolTestSupport
        let session = F.session(), hash = try F.hash(session)
        var owner = try AppUIInstallCancelAdmission(session: session, sessionSHA256: hash, now: 100)
        try T.refuses("install skip idle") { try owner.admit(F.request(.start, phase: .validating), now: 100) }
        try T.check(owner.phase == nil && owner.nextSequence == 1, "install refusal leaves admission unchanged")
        let waiting = try F.request()
        try owner.admit(waiting, now: 100)
        try T.refuses("install one in flight") { try owner.admit(waiting, now: 100) }
        let absent = AppUIInstallCancelValues(installVisible: false, startVisible: false, startEnabled: false,
            cancelVisible: false, cancelEnabled: false, stage: "", title: "")
        try owner.complete(F.reply(waiting, values: absent), now: 100)
        try T.refuses("install incomplete observation cannot advance") {
            try owner.admit(F.request(.start, phase: .validating, sequence: 2), now: 100)
        }
        try T.refuses("install cannot extend poll deadline") {
            try owner.admit(F.request(sequence: 2, deadline: 106), now: 101)
        }
        func exchange(_ operation: AppUIInstallCancelOperation, phase: AppUIInstallCancelPhase) throws {
            let request = try F.request(operation, phase: phase, sequence: owner.nextSequence)
            try owner.admit(request, now: 100.1)
            try owner.complete(F.reply(request, values: operation == .readState ? F.values(phase) : nil), now: 100.2)
        }
        try exchange(.readState, phase: .idle)
        try T.refuses("install read cannot precede start") {
            try owner.admit(F.request(phase: .validating, sequence: owner.nextSequence), now: 100.2)
        }
        try exchange(.start, phase: .validating)
        try T.refuses("install start cannot repeat") {
            try owner.admit(F.request(.start, phase: .validating, sequence: owner.nextSequence), now: 100.2)
        }
        try T.refuses("install mutation success alone cannot advance") {
            try owner.admit(F.request(.cancel, phase: .cancelling, sequence: owner.nextSequence), now: 100.2)
        }
        try exchange(.readState, phase: .validating)
        try exchange(.cancel, phase: .cancelling)
        try T.refuses("install cancel cannot repeat") {
            try owner.admit(F.request(.cancel, phase: .cancelling, sequence: owner.nextSequence), now: 100.2)
        }
        try exchange(.readState, phase: .cancelling)
        try T.refuses("install retry cannot skip cancellation settlement") {
            try owner.admit(F.request(.retry, phase: .retryFailed, sequence: owner.nextSequence), now: 100.2)
        }
        try exchange(.readState, phase: .cancelled)
        try exchange(.retry, phase: .retryFailed)
        try T.refuses("install retry cannot repeat") {
            try owner.admit(F.request(.retry, phase: .retryFailed, sequence: owner.nextSequence), now: 100.2)
        }
        try exchange(.readState, phase: .retryFailed)
        try T.check(owner.terminal && owner.pending == nil && owner.failureCode == nil && owner.nextSequence == 10,
            "install full ordered scenario plus one pending read finishes")
        try T.refuses("install terminal forbids later request") {
            try owner.admit(F.request(phase: .retryFailed, sequence: owner.nextSequence), now: 100.3)
        }
        try aborts(session, hash: hash)
    }

    private static func aborts(_ session: AppUIInstallCancelSession, hash: String) throws {
        typealias T = AppUIDriverProtocolTestSupport
        typealias F = AppUIInstallCancelProtocolTestSupport
        func fresh() throws -> AppUIInstallCancelAdmission {
            try AppUIInstallCancelAdmission(session: session, sessionSHA256: hash, now: 100)
        }
        let waiting = try F.request(), reply = try F.reply(waiting, values: F.values(.idle))
        var timed = try fresh(); try timed.admit(waiting, now: 100)
        try T.refuses("install late successful response") { try timed.complete(reply, now: 105) }
        try T.check(timed.terminal && timed.pending == nil && timed.failureCode == .deadlineExceeded,
            "install deadline closes pending admission")
        var cancelled = try fresh(); try cancelled.admit(waiting, now: 100)
        cancelled.abort(.cancelled)
        try T.check(cancelled.terminal && cancelled.pending == nil && cancelled.failureCode == .cancelled,
            "install external cancellation closes admission")
        try T.refuses("install cancelled response cannot advance") { try cancelled.complete(reply, now: 101) }
        var refused = try fresh(); try refused.admit(waiting, now: 100)
        try refused.complete(F.reply(waiting, failure: .axFailure), now: 101)
        try T.check(refused.terminal && refused.failureCode == .axFailure, "install AX refusal closes admission")
        var exhausted = try fresh()
        let absent = AppUIInstallCancelValues(installVisible: false, startVisible: false, startEnabled: false,
            cancelVisible: false, cancelEnabled: false, stage: "", title: "")
        for sequence in 1...AppUIDriverConstants.maximumSequence {
            let request = try F.request(sequence: sequence)
            try exhausted.admit(request, now: 100)
            try exhausted.complete(F.reply(request, values: absent), now: 100)
        }
        try T.refuses("install 1025th exchange") { try exhausted.admit(F.request(sequence: 1025), now: 100) }
    }
}
