import Darwin
import Foundation

/// The d6 refusal's AX failure locus on the wire. The reply validator and the
/// host's actual read path accept a locus only well formed and only with its
/// distinct count; every earlier reply, including the exact bytes of the live
/// 6121a878 refusal, stays valid; the install-cancel reply never admits one.
/// Values and owned files only: no GUI, AX message or permission query.
@main
enum AppUIDriverAXLocusProtocolContracts {
    typealias T = AppUIDriverProtocolTestSupport
    typealias L = AppUIDriverAXLocus
    static var passed = 0, failed: [String] = []

    /// Every check runs; each failure is listed.
    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) {
        if (try? condition()) == true { passed += 1; return }
        failed.append(name)
        FileHandle.standardError.write(Data("FAIL: \(name)\n".utf8))
    }

    static let locus = L(depth: 3, indexPath: [9, 0, 0], visited: 12, role: "AXCell", subrole: L.absent,
                         parentRole: "AXRow", attributeNames: ["AXChildren", "AXFrame", "AXParent", "AXRole"],
                         identifierListed: false)

    static func context(_ locus: L? = locus, distinct: Int? = 1, api: AppUIDriverAXContext.API = .copyAttributeValue,
                        attribute: AppUIDriverAXContext.Attribute = .identifier,
                        scope: AppUIDriverAXContext.Scope = .element) -> AppUIDriverAXContext {
        AppUIDriverAXContext(api: api, attribute: attribute, scope: scope, locus: locus, distinctLoci: distinct)
    }

    static func refusal(_ context: AppUIDriverAXContext?, _ request: AppUIDriverRequest,
                        session: AppUIDriverSession = T.session()) throws -> AppUIDriverReply {
        var reply = try T.reply(request, failure: .axFailure, session: session)
        reply.axContext = context
        return reply
    }

    static func valid(_ data: Data, _ request: AppUIDriverRequest, session: AppUIDriverSession = T.session()) -> Bool {
        guard let value = try? AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverReply.self),
              let hash = try? AppUIDriverCanonicalJSON.digest(AppUIDriverCanonicalJSON.encode(request)) else { return false }
        return (try? AppUIDriverValidation.reply(value, request: request, session: session, requestSHA256: hash)) != nil
    }

    static func json(_ value: [String: Any]) throws -> Data {
        try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
    }

    /// The exact request 7 and reply 7 of bvm-6121a878-native-ui-v2-20261023-02, written before the locus existed.
    static func liveRefusalStaysValid() throws {
        let nonce = "642ca11f78ffe7cf8ce749aeccef38f89c120d5762e710f355c76d28a5f92a22"
        let session = "70a38ffed7169dd676b8e83a7883b55512b65cd19b78de0334bdc4a15ff63700"
        let requestBytes = Data(("{\"kind\":\"native-app-ui-driver-request\",\"nonce\":\"\(nonce)\",\"operation\":\"pressOverview\","
            + "\"phase\":\"overview\",\"phaseDeadlineUptime\":1885478.3986207084,\"schemaVersion\":1,\"sequence\":7,"
            + "\"sessionSHA256\":\"\(session)\",\"window\":\"main\"}").utf8)
        let replyBytes = Data(("{\"axContext\":{\"api\":\"copyAttributeValue\",\"attribute\":\"AXIdentifier\",\"scope\":\"element\"},"
            + "\"axError\":-25200,\"driverPID\":58156,\"failureCode\":\"ax-failure\",\"hostPID\":58133,"
            + "\"kind\":\"native-app-ui-driver-reply\",\"nonce\":\"\(nonce)\",\"operation\":\"pressOverview\",\"outcome\":\"refused\","
            + "\"requestSHA256\":\"34c6ba2c83a7688b0458a5afaa078b9251f406fed55682c76ba0be7d40d22842\",\"schemaVersion\":1,"
            + "\"sequence\":7,\"sessionSHA256\":\"\(session)\"}").utf8)
        check(requestBytes.count == 332 && replyBytes.count == 521, "the live request and reply sizes")
        let request = try AppUIDriverCanonicalJSON.decode(requestBytes, as: AppUIDriverRequest.self)
        check(AppUIDriverCanonicalJSON.digest(requestBytes) == "34c6ba2c83a7688b0458a5afaa078b9251f406fed55682c76ba0be7d40d22842",
              "the live request digest is the reply's requestSHA256")
        let fixture = T.session()
        let live = AppUIDriverSession(nonce: nonce, startedUptime: fixture.startedUptime, deadlineUptime: fixture.deadlineUptime,
            host: .init(pid: 58133, launchDate: 1, bundleIdentifier: "", bundlePath: "", executablePath: "", executableSHA256: ""),
            driver: .init(pid: 58156, launchDate: 1, bundleIdentifier: "", bundlePath: "", executablePath: "", executableSHA256: ""))
        let reply = try AppUIDriverCanonicalJSON.decode(replyBytes, as: AppUIDriverReply.self)
        check(reply.axContext?.locus == nil && reply.axContext?.distinctLoci == nil, "the live refusal has no locus")
        check(try AppUIDriverCanonicalJSON.encode(reply) == replyBytes, "the live refusal re-encodes byte for byte")
        check(valid(replyBytes, request, session: live), "the live 6121a878 refusal is still a valid reply")
    }

    static func earlierRepliesStayValid() throws {
        let request = try T.request(.pressOverview, phase: .overview)
        let bare = try AppUIDriverCanonicalJSON.encode(T.reply(request, failure: .axFailure))
        check(valid(bare, request) && !String(decoding: bare, as: UTF8.self).contains("axContext"),
              "a context-free AX refusal stays valid and unchanged")
        let unlocated = try AppUIDriverCanonicalJSON.encode(refusal(context(nil, distinct: nil), request))
        let text = String(decoding: unlocated, as: UTF8.self)
        check(valid(unlocated, request) && !text.contains("locus") && !text.contains("distinctLoci"),
              "a refusal outside the pass keeps its context without locus keys")
        for failure in [AppUIDriverFailure.elementNotFound, .deadlineExceeded, .cancelled] {
            check(valid(try AppUIDriverCanonicalJSON.encode(T.reply(request, failure: failure)), request),
                  "a non-AX refusal stays valid")
        }
        let observed = try T.request()
        check(valid(try AppUIDriverCanonicalJSON.encode(T.reply(observed, values: .init(createVisible: true, importVisible: true))),
                    observed), "an observation stays valid")
    }

    static func main() {
        do {
            try liveRefusalStaysValid()
            try earlierRepliesStayValid()
            try locatedRepliesAreExact()
            try malformedLociAreRefused()
            try hostReadPath()
            try installCancelRefusesALocus()
        } catch {
            failed.append("fixture: \(error)")
        }
        guard failed.isEmpty else {
            FileHandle.standardError.write(Data("FAIL: \(failed.count) of \(passed + failed.count) AX locus protocol checks\n".utf8))
            exit(1)
        }
        print("PASS: \(passed) AX failure locus protocol checks; no GUI, AX message or permission query")
    }
}
