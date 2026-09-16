import Foundation

enum AppUIDriverAdmissionContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        let session = T.session(), hash = try T.hash(session)
        func fresh() throws -> AppUIDriverAdmission { try AppUIDriverAdmission(session: session, sessionSHA256: hash, now: 100) }
        var owner = try fresh()
        try T.refuses("skip welcome phase") { try owner.admit(T.request(.pressCreate, phase: .creation), now: 100) }
        try T.check(owner.phase == nil && owner.nextSequence == 1, "refusal leaves admission unchanged")
        let waiting = try T.request()
        try owner.admit(waiting, now: 100)
        try T.refuses("one in flight") { try owner.admit(waiting, now: 100) }
        try owner.complete(T.reply(waiting, values: .init(createVisible: false, importVisible: true)), now: 100)
        try T.refuses("false welcome cannot advance") {
            try owner.admit(T.request(.pressCreate, phase: .creation, sequence: 2), now: 100)
        }
        try T.refuses("phase deadline cannot extend on later poll") {
            try owner.admit(T.request(sequence: 2, deadline: 106), now: 101)
        }
        let ready = try T.request(sequence: 2)
        try owner.admit(ready, now: 100.1)
        try T.refuses("wrong reply request hash") {
            try owner.complete(T.reply(T.request(sequence: 1), values: .init(createVisible: true, importVisible: true)), now: 100.1)
        }
        try owner.complete(T.reply(ready, values: .init(createVisible: true, importVisible: true)), now: 100.2)
        try T.refuses("query cannot precede phase mutation") {
            try owner.admit(T.request(.createForm, phase: .creation, sequence: 3), now: 100.2)
        }
        try T.check(owner.phase == .welcome, "wrong first operation leaves previous phase")
        func exchange(_ operation: AppUIDriverOperation, phase: AppUIDriverPhase, values: AppUIDriverValues? = nil) throws {
            let request = try T.request(operation, phase: phase, sequence: owner.nextSequence)
            try owner.admit(request, now: 100.2)
            try owner.complete(T.reply(request, values: values), now: 100.3)
        }
        try exchange(.pressCreate, phase: .creation)
        try T.refuses("mutation never repeats") {
            try owner.admit(T.request(.pressCreate, phase: .creation, sequence: owner.nextSequence), now: 100.3)
        }
        try exchange(.createForm, phase: .creation, values: .init(commitVisible: false))
        try T.refuses("incomplete form cannot advance") {
            try owner.admit(T.request(.cancelCreate, phase: .cancellation, sequence: owner.nextSequence), now: 100.3)
        }
        try exchange(.createForm, phase: .creation, values: .init(commitVisible: true,
            isoSelection: "", payloadSelection: "", manifestSelection: ""))
        try exchange(.cancelCreate, phase: .cancellation)
        try exchange(.pressImport, phase: .importing)
        try exchange(.importForm, phase: .importing, values: .init(nameVisible: true))
        try exchange(.pressOverview, phase: .overview)
        try exchange(.overviewCards, phase: .overview, values: .init(amberVisible: true, indigoVisible: true))
        try exchange(.setSearchIndigo, phase: .filtering, values: .init(searchValue: "Indigo"))
        try exchange(.searchState, phase: .filtering, values: .init(amberVisible: false, indigoVisible: true, searchValue: "Indigo"))
        try exchange(.clearSearch, phase: .clearing)
        try exchange(.searchState, phase: .clearing, values: .init(amberVisible: true, indigoVisible: true, searchValue: ""))
        try T.check(owner.terminal && owner.pending == nil, "full ordered scenario finishes")
        try T.refuses("terminal forbids later request") {
            try owner.admit(T.request(.searchState, phase: .clearing, sequence: owner.nextSequence), now: 100.4)
        }
        var timed = try fresh(); try timed.admit(waiting, now: 100)
        try T.refuses("late successful response") {
            try timed.complete(T.reply(waiting, values: .init(createVisible: true, importVisible: true)), now: 105)
        }
        try T.check(timed.terminal, "deadline closes admission")
        var refused = try fresh(); try refused.admit(waiting, now: 100)
        try refused.complete(T.reply(waiting, failure: .axFailure), now: 101)
        try T.check(refused.terminal, "actual AX failure closes admission")
        var exhausted = try fresh()
        for sequence in 1...AppUIDriverConstants.maximumSequence {
            let request = try T.request(sequence: sequence)
            try exhausted.admit(request, now: 100)
            try exhausted.complete(T.reply(request, values: .init(createVisible: false, importVisible: false)), now: 100)
        }
        try T.refuses("1025th exchange") { try exhausted.admit(T.request(sequence: 1025), now: 100) }
        try validationFaults(session, hash: hash)
    }

    private static func validationFaults(_ session: AppUIDriverSession, hash: String) throws {
        typealias T = AppUIDriverProtocolTestSupport
        let original = try T.request()
        func invalid(nonce: String? = nil, window: AppUIDriverWindow = .main, deadline: Double = 105) -> AppUIDriverRequest {
            AppUIDriverRequest(sessionSHA256: hash, nonce: nonce ?? session.nonce, sequence: 1,
                phase: .welcome, phaseDeadlineUptime: deadline, window: window, operation: .welcomeControls)
        }
        for request in [invalid(nonce: String(repeating: "d", count: 64)), invalid(window: .creationSheet),
                        invalid(deadline: .infinity), invalid(deadline: 100), invalid(deadline: 105.1)] {
            try T.refuses("invalid nonce/window/deadline") {
                try AppUIDriverValidation.request(request, session: session, sessionSHA256: hash, now: 100)
            }
        }
        let reply = try T.reply(original, values: .init(createVisible: true, importVisible: true, searchValue: "unexpected"))
        try T.refuses("extra query result field") {
            try AppUIDriverValidation.reply(reply, request: original, session: session,
                requestSHA256: AppUIDriverCanonicalJSON.digest(AppUIDriverCanonicalJSON.encode(original)))
        }
        let set = try T.request(.setSearchIndigo, phase: .filtering)
        try T.refuses("wrong search readback") {
            try AppUIDriverValidation.reply(T.reply(set, values: .init(searchValue: "Amber")), request: set,
                session: session, requestSHA256: AppUIDriverCanonicalJSON.digest(AppUIDriverCanonicalJSON.encode(set)))
        }
        let form = try T.request(.createForm, phase: .creation)
        try T.refuses("visible commit cannot invent missing selections") {
            try AppUIDriverValidation.reply(T.reply(form, values: .init(commitVisible: true)), request: form,
                session: session, requestSHA256: AppUIDriverCanonicalJSON.digest(AppUIDriverCanonicalJSON.encode(form)))
        }
        let wrongPID = AppUIDriverReply(sessionSHA256: hash, nonce: session.nonce, sequence: 1,
            requestSHA256: AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(original)),
            hostPID: 33, driverPID: session.driver.pid, operation: .welcomeControls, outcome: .observed,
            values: .init(createVisible: true, importVisible: true), failureCode: nil, axError: nil)
        try T.refuses("reply wrong process") {
            try AppUIDriverValidation.reply(wrongPID, request: original, session: session, requestSHA256: wrongPID.requestSHA256)
        }
    }
}
