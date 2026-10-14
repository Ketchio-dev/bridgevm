import Foundation

enum AppUIInstallCancelCanonicalContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        typealias F = AppUIInstallCancelProtocolTestSupport
        let session = F.session(), data = try AppUIDriverCanonicalJSON.encode(session)
        try T.check(try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelSession.self) == session,
            "install canonical roundtrip")
        let original = String(decoding: data, as: UTF8.self)
        for replacement in ["\"schemaVersion\":1,\"schemaVersion\":1", "\"schemaVersion\":1,\"unknown\":true"] {
            try T.refuses("install duplicate or unknown field") {
                _ = try AppUIDriverCanonicalJSON.decode(Data(original.replacingOccurrences(
                    of: "\"schemaVersion\":1", with: replacement).utf8), as: AppUIInstallCancelSession.self)
            }
        }
        let legacy = try AppUIDriverCanonicalJSON.encode(T.session())
        try T.refuses("v2 session cannot enter install protocol") {
            try AppUIInstallCancelValidation.session(AppUIDriverCanonicalJSON.decode(legacy, as: AppUIInstallCancelSession.self))
        }
        try T.refuses("install session cannot enter v2 protocol") {
            try AppUIDriverValidation.session(AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverSession.self))
        }
        let request = try F.request(), requestData = try AppUIDriverCanonicalJSON.encode(request)
        let requestText = String(decoding: requestData, as: UTF8.self)
        for (old, new) in [("\"readState\"", "\"pressCreate\""), ("\"idle\"", "\"welcome\"")] {
            try T.refuses("v2 operation or phase cannot enter install request") {
                _ = try AppUIDriverCanonicalJSON.decode(Data(requestText.replacingOccurrences(of: old, with: new).utf8),
                    as: AppUIInstallCancelRequest.self)
            }
        }
        try T.refuses("install request cannot decode as v2") {
            _ = try AppUIDriverCanonicalJSON.decode(requestData, as: AppUIDriverRequest.self)
        }
        let values = try AppUIDriverCanonicalJSON.encode(F.values(.idle))
        let valuesText = String(decoding: values, as: UTF8.self)
        for replacement in ["", "\"startEnabled\":1,", "\"startEnabled\":true,\"unknown\":true,"] {
            try T.refuses("install mandatory Boolean and exact values fields") {
                _ = try AppUIDriverCanonicalJSON.decode(Data(valuesText.replacingOccurrences(
                    of: "\"startEnabled\":true,", with: replacement).utf8), as: AppUIInstallCancelValues.self)
            }
        }
        var malformed = session; malformed.kind = "native-app-ui-driver-session"
        try T.refuses("install wrong kind") { try AppUIInstallCancelValidation.session(malformed) }
        let tooLong = AppUIInstallCancelSession(nonce: session.nonce, startedUptime: 100, deadlineUptime: 191,
            host: session.host, driver: session.driver)
        try T.refuses("install cannot extend90s") { try AppUIInstallCancelValidation.session(tooLong) }
        try T.refuses("install phase cannot exceed5s") {
            try AppUIInstallCancelValidation.request(F.request(deadline: 105.1), session: session,
                sessionSHA256: F.hash(session), now: 100)
        }
        try T.refuses("install stale global request") {
            try AppUIInstallCancelValidation.request(request, session: session, sessionSHA256: F.hash(session), now: 190)
        }
        let untrusted = try F.ready(session, trusted: false)
        try AppUIInstallCancelValidation.ready(untrusted, session: session, sessionSHA256: F.hash(session))
        try T.check(untrusted.failureCode == .accessibilityUntrusted, "install explicit untrusted result")
        try replyValues(session, request: request)
    }

    private static func replyValues(_ session: AppUIInstallCancelSession, request: AppUIInstallCancelRequest) throws {
        typealias T = AppUIDriverProtocolTestSupport
        typealias F = AppUIInstallCancelProtocolTestSupport
        let hash = AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(request))
        try T.refuses("install state needs mandatory values") {
            try AppUIInstallCancelValidation.reply(F.reply(request), request: request, session: session, requestSHA256: hash)
        }
        var values = F.values(.retryFailed); values.failure = "unexpected failure"
        try T.refuses("install arbitrary failure text refused") {
            try AppUIInstallCancelValidation.reply(F.reply(request, values: values), request: request,
                session: session, requestSHA256: hash)
        }
        for phase in AppUIInstallCancelPhase.allCases {
            let read = try F.request(phase: phase)
            let reply = try F.reply(read, values: F.values(phase))
            try T.check(AppUIInstallCancelOperationPolicy.completes(phase, reply: reply), "install exact phase state passes")
            if phase != .idle {
                try T.check(!AppUIInstallCancelOperationPolicy.completes(phase,
                    reply: F.reply(read, values: F.values(.idle))), "install stale idle cannot complete another phase")
            }
            if let mutation = AppUIInstallCancelOperationPolicy.mutation(for: phase) {
                let action = try F.request(mutation, phase: phase)
                try T.check(!AppUIInstallCancelOperationPolicy.completes(phase, reply: F.reply(action)),
                    "install AX press alone cannot prove phase state")
            }
        }
    }
}
