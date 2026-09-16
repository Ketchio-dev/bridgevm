import Foundation
import Security

enum AppUIDriverCanonicalContracts {
    static func run() throws {
        typealias T = AppUIDriverProtocolTestSupport
        let session = T.session()
        let data = try AppUIDriverCanonicalJSON.encode(session)
        try T.check(try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverSession.self) == session, "canonical roundtrip")
        let original = String(decoding: data, as: UTF8.self)
        func mutate(_ old: String, _ new: String, _ message: String) throws {
            try T.check(original.contains(old), "counterexample target exists: " + message)
            let mutated = Data(original.replacingOccurrences(of: old, with: new).utf8)
            try T.refuses(message) { _ = try AppUIDriverCanonicalJSON.decode(mutated, as: AppUIDriverSession.self) }
        }
        try mutate("\"schemaVersion\":1", "\"schemaVersion\":1,\"schemaVersion\":1", "duplicate equal key")
        try mutate("\"schemaVersion\":1", "\"schemaVersion\":2,\"schemaVersion\":1", "duplicate unequal key")
        try mutate("\"schemaVersion\":1", "\"schemaVersion\":1,\"schem\\u0061Version\":1", "escaped duplicate key")
        try mutate("\"schemaVersion\":1", "\"schemaVersion\":1,\"unknown\":true", "unknown key")
        try mutate("\"startedUptime\":100", "\"startedUptime\":100.0", "alternate number")
        try T.refuses("trailing newline") { _ = try AppUIDriverCanonicalJSON.decode(data + Data([10]), as: AppUIDriverSession.self) }
        try T.refuses("empty data") { _ = try AppUIDriverCanonicalJSON.decode(Data(), as: AppUIDriverSession.self) }
        try T.refuses("over8KiB decode") { _ = try AppUIDriverCanonicalJSON.decode(Data(repeating: 32, count: 8193), as: AppUIDriverSession.self) }
        try T.refuses("over8KiB encode") { _ = try AppUIDriverCanonicalJSON.encode(AppUIDriverValues(searchValue: String(repeating: "x", count: 8192))) }
        try T.refuses("failed CSPRNG") { _ = try AppUIDriverValidation.nonce(bytes: [UInt8](repeating: 0, count: 32), providerStatus: errSecNotAvailable) }
        try T.refuses("short CSPRNG") { _ = try AppUIDriverValidation.nonce(bytes: [0], providerStatus: errSecSuccess) }
        try T.check(AppUIDriverValidation.hexDigest(try AppUIDriverValidation.makeNonce()), "real CSPRNG shape")
        try AppUIDriverValidation.session(session)
        var malformed = session; malformed.schemaVersion = 2
        try T.refuses("wrong schema") { try AppUIDriverValidation.session(malformed) }
        let tooLong = AppUIDriverSession(nonce: session.nonce, startedUptime: 100, deadlineUptime: 191,
            host: session.host, driver: session.driver)
        try T.refuses("global deadline extended") { try AppUIDriverValidation.session(tooLong) }
        let untrusted = try T.ready(session, trusted: false)
        try AppUIDriverValidation.ready(untrusted, session: session, sessionSHA256: T.hash(session))
        try T.check(untrusted.failureCode == .accessibilityUntrusted, "untrusted explicit failure")
        let falseReady = AppUIDriverReady(sessionSHA256: try T.hash(session), nonce: session.nonce,
            driver: session.driver, trusted: true, failureCode: .accessibilityUntrusted,
            codeRequirement: nil, codeRequirementUnavailableReason: "not-observed")
        try T.refuses("trust flags disagree") { try AppUIDriverValidation.ready(falseReady, session: session, sessionSHA256: T.hash(session)) }
        let unknownCode = AppUIDriverReady(sessionSHA256: try T.hash(session), nonce: session.nonce,
            driver: session.driver, trusted: true, failureCode: nil, codeRequirement: nil, codeRequirementUnavailableReason: nil)
        try T.refuses("unobserved code requires reason") { try AppUIDriverValidation.ready(unknownCode, session: session, sessionSHA256: T.hash(session)) }
    }
}
