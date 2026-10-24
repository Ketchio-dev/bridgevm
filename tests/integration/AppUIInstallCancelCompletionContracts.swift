import Foundation

enum AppUIInstallCancelCompletionContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        typealias F = AppUIInstallCancelProtocolTestSupport
        let session = F.session(), hash = try F.hash(session)
        for (requests, mutations) in [(8, 3), (1024, 3)] {
            try AppUIInstallCancelValidation.completion(F.completion(session, requests: requests, mutations: mutations),
                session: session, sessionSHA256: hash)
        }
        for (requests, mutations) in [(7, 3), (8, 2), (9, 4), (1025, 3), (-1, 0)] {
            try T.refuses("install success needs exactly3 mutations and8...1024 requests") {
                try AppUIInstallCancelValidation.completion(F.completion(session, requests: requests, mutations: mutations),
                    session: session, sessionSHA256: hash)
            }
        }
        try AppUIInstallCancelValidation.completion(F.completion(session, success: false, requests: 0, mutations: 0),
            session: session, sessionSHA256: hash)
        let legacyCompletion = AppUIDriverCompletion(sessionSHA256: hash, nonce: session.nonce, hostPID: session.host.pid,
            driverPID: session.driver.pid, success: false, failureCode: nil, requestsProcessed: 8, mutationsPerformed: 3, axRestarts: 0)
        try T.refuses("v2 completion cannot satisfy install kind") {
            let decoded = try AppUIDriverCanonicalJSON.decode(AppUIDriverCanonicalJSON.encode(legacyCompletion),
                as: AppUIInstallCancelCompletion.self)
            try AppUIInstallCancelValidation.completion(decoded, session: session, sessionSHA256: hash)
        }
        func host(success: Bool = true, settled: Bool = true, preserved: Bool = true,
                  pid: Int32? = nil, failure: AppUIDriverFailure? = nil) -> AppUIInstallCancelHostCompletion {
            AppUIInstallCancelHostCompletion(pid: pid ?? session.host.pid, success: success, settled: settled, fixturePreserved: preserved,
                reportSHA256: String(repeating: "a", count: 64), failureCode: failure)
        }
        try AppUIInstallCancelValidation.hostCompletion(host(), session: session)
        for invalid in [host(settled: false), host(preserved: false), host(pid: session.driver.pid), host(failure: .cancelled)] {
            try T.refuses("install host success needs identity settlement preservation and no failure") {
                try AppUIInstallCancelValidation.hostCompletion(invalid, session: session)
            }
        }
        try AppUIInstallCancelValidation.hostCompletion(host(success: false, settled: false, preserved: false, failure: .cancelled),
            session: session)
        let data = try AppUIDriverCanonicalJSON.encode(host())
        let original = String(decoding: data, as: UTF8.self)
        for replacement in ["\"success\":1", "\"success\":true,\"unknown\":false", "\"success\":true,\"success\":true"] {
            try T.refuses("install host canonical Boolean duplicate unknown refusal") {
                _ = try AppUIDriverCanonicalJSON.decode(Data(original.replacingOccurrences(of: "\"success\":true", with: replacement).utf8),
                    as: AppUIInstallCancelHostCompletion.self)
            }
        }
        var wrongKind = host(); wrongKind.kind = "native-app-ui-host-completion"
        try T.refuses("legacy host completion kind cannot satisfy install completion") {
            try AppUIInstallCancelValidation.hostCompletion(wrongKind, session: session)
        }
        try T.refuses("install host completion8KiB bound") {
            _ = try AppUIDriverCanonicalJSON.decode(Data(repeating: 32, count: 8193), as: AppUIInstallCancelHostCompletion.self)
        }
    }
}
