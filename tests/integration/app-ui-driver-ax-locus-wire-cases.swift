import Foundation

extension AppUIDriverAXLocusProtocolContracts {
    /// Every shape the driver can write is accepted exactly, up to each bound.
    static func locatedRepliesAreExact() throws {
        let request = try T.request(.pressOverview, phase: .overview)
        func accepts(_ context: AppUIDriverAXContext, _ name: String) throws {
            let data = try AppUIDriverCanonicalJSON.encode(refusal(context, request))
            let decoded = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverReply.self)
            check(decoded.axContext == context && valid(data, request), name)
        }
        try accepts(context(), "a nested element's locus round trips through reply validation")
        let application = L(depth: -1, indexPath: [], visited: 0, role: "AXApplication", subrole: L.absent,
                            parentRole: L.absent, attributeNames: [], identifierListed: false)
        try accepts(context(application, api: .copyAttributeValues, attribute: .windows), "the application in the window lookup")
        let window = L(depth: -1, indexPath: [2], visited: 0, role: L.unreadable, subrole: L.unreadable,
                       parentRole: "AXApplication", attributeNames: ["AXIdentifier", "AXRole"], identifierListed: true)
        try accepts(context(window), "a window lookup entry, with unreadable fields")
        let main = L(depth: 0, indexPath: [], visited: 0, role: "AXWindow", subrole: "AXStandardWindow",
                     parentRole: "AXApplication", attributeNames: [], identifierListed: false)
        try accepts(context(main, api: .getAttributeValueCount, attribute: .children), "the main window's own children read")
        try accepts(context(main, api: .getPID, attribute: .children, scope: .relatedElement), "a child's preparation")
        let deep = L(depth: 40, indexPath: Array(repeating: 1, count: 32), visited: 40, role: "AXGroup", subrole: L.absent,
                     parentRole: "AXGroup", attributeNames: ["AXRole"], identifierListed: false)
        try accepts(context(deep, distinct: L.maximumDistinct), "a path past 32 steps, with the most distinct loci")
        // 64 names that all sort before AXIdentifier: the bound dropped it, so it is listed but not kept.
        let letters = Array("ABCDEFGH")
        let names = letters.flatMap { first in letters.map { "AX\(first)\($0)" + String(repeating: "z", count: 38) } }
        let widest = L(depth: L.maximumVisited, indexPath: Array(repeating: 9_999, count: 32), visited: L.maximumVisited,
                       role: "AX" + String(repeating: "R", count: 40), subrole: "AX" + String(repeating: "s", count: 40),
                       parentRole: "AX" + String(repeating: "P", count: 40), attributeNames: names, identifierListed: true)
        check(names.count == 64 && names == names.sorted(), "the widest fixture keeps 64 sorted names")
        try accepts(context(widest, distinct: L.maximumDistinct), "the widest locus, identifier past the 64th name")
        let bytes = try AppUIDriverCanonicalJSON.encode(refusal(context(widest, distinct: L.maximumDistinct), request))
        check(bytes.count <= AppUIDriverConstants.maximumBytes, "the widest located refusal stays within 8 KiB (\(bytes.count) bytes)")
        for listed in ["AXIdentifier", "AXRole"] {
            let kept = L(depth: 1, indexPath: [0], visited: 1, role: "AXRow", subrole: "AXOutlineRow", parentRole: "AXOutline",
                         attributeNames: [listed], identifierListed: listed == "AXIdentifier")
            try accepts(context(kept), "identifierListed follows the names (\(listed))")
        }
    }

    /// Each change makes the record malformed; decoding or validation refuses every one.
    static func malformedLociAreRefused() throws {
        let request = try T.request(.pressOverview, phase: .overview)
        let base = try JSONSerialization.jsonObject(with: AppUIDriverCanonicalJSON.encode(refusal(context(), request))) as! [String: Any]
        func refuses(_ name: String, _ change: (inout [String: Any], inout [String: Any]) -> Void) throws {
            var value = base, axContext = base["axContext"] as! [String: Any], locus = axContext["locus"] as! [String: Any]
            change(&axContext, &locus)
            if !locus.isEmpty { axContext["locus"] = locus }
            value["axContext"] = axContext
            check(!valid(try json(value), request), name)
        }
        check(valid(try json(base), request), "the unchanged serialized locus is valid")
        try refuses("a locus without its distinct count") { c, _ in c.removeValue(forKey: "distinctLoci") }
        try refuses("a distinct count without a locus") { c, l in c.removeValue(forKey: "locus"); l = [:] }
        for count: Any in [0, -1, L.maximumDistinct + 1, "1", 1.5, true] {
            try refuses("distinct count \(count)") { c, _ in c["distinctLoci"] = count }
        }
        for api in ["performAction", "setAttributeValue", "isAttributeSettable"] {
            try refuses("a mutation's \(api) never has a locus") { c, _ in c["api"] = api }
        }
        for attribute in ["none", "other"] {
            try refuses("attribute \(attribute) is no pass read") { c, _ in c["attribute"] = attribute }
        }
        try refuses("an unknown context key") { c, _ in c["path"] = "/private" }
        for (key, bad) in [("depth", -2), ("depth", 2), ("depth", 4), ("visited", -1), ("visited", 2),
                           ("visited", L.maximumVisited + 1)] as [(String, Any)] {
            try refuses("\(key) \(bad)") { _, l in l[key] = bad }
        }
        try refuses("a path entry past 9999") { _, l in l["indexPath"] = [9, 10_000, 0] }
        try refuses("a negative path entry") { _, l in l["indexPath"] = [9, -1, 0] }
        func place(_ depth: Int, _ visited: Int, _ path: [Int]) -> (inout [String: Any], inout [String: Any]) -> Void {
            { _, l in l["depth"] = depth; l["visited"] = visited; l["indexPath"] = path }
        }
        try refuses("a path longer than 32", place(33, 33, Array(repeating: 0, count: 33)))
        try refuses("a truncated path shorter than 32", place(40, 40, Array(repeating: 0, count: 31)))
        try refuses("a window lookup with two steps", place(-1, 0, [0, 1]))
        try refuses("a window lookup after the walk", place(-1, 1, [0]))
        try refuses("the application with a parent", place(-1, 0, []))
        for key in ["role", "subrole", "parentRole"] {
            for bad in ["AXCell1", "AX", "AX" + String(repeating: "a", count: 41), "Cell", "axCell", "AXCé", "AX Cell",
                        "unreadable ", "Unreadable", "", "private UI text"] {
                try refuses("\(key) '\(bad)'") { _, l in l[key] = bad }
            }
        }
        try refuses("unsorted names") { _, l in l["attributeNames"] = ["AXRole", "AXFrame"] }
        try refuses("duplicate names") { _, l in l["attributeNames"] = ["AXRole", "AXRole"] }
        try refuses("a name that is no AX constant") { _, l in l["attributeNames"] = ["AXFrame", "title"] }
        try refuses("65 names") { _, l in l["attributeNames"] = constants("AXN", 65) }
        try refuses("identifier kept but not listed") { _, l in l["attributeNames"] = ["AXIdentifier"] }
        try refuses("identifier listed but neither kept nor cut") { _, l in l["identifierListed"] = true }
        try refuses("identifier listed beside 64 names it sorts before") { _, l in
            l["attributeNames"] = constants("AXZ", 64)
            l["identifierListed"] = true
        }
        try refuses("an unknown locus key") { _, l in l["title"] = "private UI text" }
        for key in ["depth", "indexPath", "visited", "role", "subrole", "parentRole", "attributeNames", "identifierListed"] {
            try refuses("a locus without \(key)") { _, l in l.removeValue(forKey: key) }
        }
        try refuses("a boolean as text") { _, l in l["identifierListed"] = "false" }
        try refuses("names as text") { _, l in l["attributeNames"] = "AXRole" }
        var value = base
        value["failureCode"] = "element-not-found"
        check(!valid(try json(value), request), "a non-AX refusal cannot carry a locus")
    }

    /// `count` distinct, sorted AX constants that all start with `prefix`.
    static func constants(_ prefix: String, _ count: Int) -> [String] {
        (0..<count).map { prefix + String(repeating: "a", count: $0 / 26) + String(UnicodeScalar(UInt8(97 + $0 % 26))) }.sorted()
    }

    /// The host's own read path: the private mailbox decodes the reply and the admission validates it.
    static func hostReadPath() throws {
        func admitted(_ locus: L) throws -> Bool {
            let root = try T.temporaryRoot()
            defer { try? FileManager.default.removeItem(at: root.deletingLastPathComponent()) }
            try AppUIDriverMailbox.prepare(privateRoot: root)
            let box = try AppUIDriverMailbox(privateRoot: root)
            let session = T.session(root: root.path)
            var admission = try AppUIDriverAdmission(session: session, sessionSHA256: try box.publishSession(session), now: 100)
            let request = try T.request(session: session)
            try admission.admit(request, now: 100)
            _ = try box.publishRequest(request)
            try box.publishReply(refusal(context(locus), request, session: session))
            guard let read = try box.readReply(sequence: 1) else { return false }
            check(read.axContext?.locus == locus && read.axContext?.distinctLoci == 1, "the host reads the locus back exactly")
            do { try admission.complete(read, now: 101) } catch { return false }
            return admission.terminal
        }
        check(try admitted(locus), "the host admits a located refusal as the request's terminal reply")
        let mismatched = L(depth: 3, indexPath: [9, 0], visited: 12, role: "AXCell", subrole: L.absent, parentRole: "AXRow",
                           attributeNames: [], identifierListed: false)
        check(try !admitted(mismatched), "the host refuses a malformed locus as an invalid reply")
    }

    static func installCancelRefusesALocus() throws {
        typealias I = AppUIInstallCancelProtocolTestSupport
        let request = try I.request()
        var reply = try I.reply(request, failure: .axFailure)
        func valid() -> Bool {
            (try? AppUIInstallCancelValidation.reply(reply, request: request, session: I.session(),
                                                     requestSHA256: reply.requestSHA256)) != nil
        }
        reply.axContext = context(nil, distinct: nil)
        check(valid(), "an install-cancel refusal keeps its context")
        reply.axContext = context()
        check(!valid(), "the install-cancel reply never admits a locus: its driver records none")
        reply.axContext = context(nil, distinct: 1)
        check(!valid(), "nor a distinct count")
    }
}
