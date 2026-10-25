import Darwin
import Foundation

/// The d6 driver's refusal says which element's AX call failed in the read-only
/// pass, through the service's wiring (AppUIDriverAXLocusAccess around the AX
/// access, under the request's restart). Injected graphs, names and clocks only:
/// no GUI, AX message or permission query. Every check runs; each failure is listed.
@main
enum AppUIDriverAXLocusContracts {
    typealias A = AppUIDriverAXAttribute
    typealias L = AppUIDriverAXLocus
    typealias Fake = AppUIDriverAXLocusFake
    typealias T = AppUIDriverProtocolTestSupport
    static var passed = 0, failed: [String] = []

    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) {
        if (try? condition()) == true { passed += 1; return }
        failed.append(name)
        FileHandle.standardError.write(Data("FAIL: \(name)\n".utf8))
    }

    /// One request as the service runs it; `located: false` is the same request without the locus recorder.
    static func run(_ fake: Fake, _ operation: AppUIDriverOperation, located: Bool = true)
        -> (result: AppUIDriverAXResult?, error: Error?, restarts: Int) {
        let restart = fake.restart()
        do {
            let result = located
                ? try AppUIDriverAXOperations(access: AppUIDriverAXLocusAccess(fake, names: fake.attributeNames),
                                              restart: restart).perform(operation)
                : try AppUIDriverAXOperations(access: fake, restart: restart).perform(operation)
            return (result, nil, restart.count)
        } catch { return (nil, error, restart.count) }
    }

    static func refusal(_ error: Error?) -> AppUIDriverAXError? { error as? AppUIDriverAXError }

    static func context(_ api: AppUIDriverAXContext.API, _ attribute: String, _ locus: L?, _ distinct: Int?) -> AppUIDriverAXContext {
        AppUIDriverAXContext(api: api, attribute: .init(attribute), scope: .element, locus: locus, distinctLoci: distinct)
    }

    /// The refusal the service publishes for this error, decoded and validated as the host does.
    static func admitted(_ error: Error?, _ operation: AppUIDriverOperation) -> Bool {
        guard let error = refusal(error), let request = try? T.request(operation),
              let hash = try? AppUIDriverCanonicalJSON.digest(AppUIDriverCanonicalJSON.encode(request)) else { return false }
        let session = T.session()
        let reply = AppUIDriverReply(sessionSHA256: request.sessionSHA256, nonce: request.nonce, sequence: request.sequence,
            requestSHA256: hash, hostPID: session.host.pid, driverPID: session.driver.pid, operation: operation,
            outcome: .refused, values: nil, failureCode: error.failure, axError: error.rawValue, axContext: error.context)
        guard let data = try? AppUIDriverCanonicalJSON.encode(reply),
              let decoded = try? AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverReply.self), decoded == reply else { return false }
        return (try? AppUIDriverValidation.reply(decoded, request: request, session: session, requestSHA256: hash)) != nil
    }

    /// The live shape: two rows' worth of nesting under the main window, below the existing controls.
    /// Node 1's children are 2...10 and 20, so 20 is index 9; 21 is 20's only child and 22 is 21's.
    static func nested() -> Fake {
        let fake = Fake()
        let graph = fake.graph
        graph.nodes[0]!.strings[A.role] = "AXApplication"
        graph.nodes[1]!.strings["AXSubrole"] = "AXStandardWindow"
        graph.add(20, role: "AXGroup"); graph.add(21, role: "AXRow"); graph.add(22, role: "AXCell")
        graph.nodes[21]!.strings["AXSubrole"] = "AXOutlineRow"
        graph.nodes[1]!.related[A.children]!.append(20)
        graph.nodes[20]!.related[A.children] = [21]
        graph.nodes[21]!.related[A.children] = [22]
        fake.names[22] = ["AXRole", "AXParent", "AXChildren", "AXFrame", "AXRole", "_AXPrivate", "NSAccessibilityTitle",
                          "AX1", "AXSize"]
        return fake
    }

    /// The walk visits 1, 20, 21, 22 and 10...3 (12 elements) before the overview lookup reads AXIdentifier
    /// of 1, 20, 21 and then 22, where -25200 persists until no restart fits.
    static let cell = L(depth: 3, indexPath: [9, 0, 0], visited: 12, role: "AXCell", subrole: L.absent, parentRole: "AXRow",
                        attributeNames: ["AXChildren", "AXFrame", "AXParent", "AXRole", "AXSize"], identifierListed: false)

    static func persistentNestedIdentifier() {
        let fake = nested()
        fake.deadline = fake.time + 0.11
        fake.faults = [.init(node: 22, attribute: A.identifier, status: -25200)]
        let outcome = run(fake, .pressOverview)
        check(refusal(outcome.error)?.rawValue == -25200 && refusal(outcome.error)?.failure == .axFailure,
              "the persistent status is refused ax-failure -25200")
        check(refusal(outcome.error)?.context == context(.copyAttributeValue, A.identifier, cell, 1),
              "the refusal carries the cell's exact locus with distinctLoci 1")
        check(outcome.restarts == 4 && fake.pass == 5 && fake.presses == 0, "the restarts are unchanged and nothing is pressed")
        check(admitted(outcome.error, .pressOverview), "the located refusal is a valid reply for the host")
        let listed = nested()
        listed.names[22]!.append("AXIdentifier")
        listed.faults = fake.faults
        let named = refusal(run(listed, .pressOverview).error)?.context?.locus
        check(named?.identifierListed == true && named?.attributeNames == ["AXChildren", "AXFrame", "AXIdentifier", "AXParent",
              "AXRole", "AXSize"], "an AXIdentifier the element lists is kept and flagged")
    }

    static func unreadableFieldsStayRefusedWithTheFailingStatus() {
        let fake = nested()
        // The walk's own role read of 22 succeeds; the locus's later read of it fails in every pass.
        fake.faults = [.init(node: 22, attribute: A.identifier, status: -25200),
                       .init(node: 22, attribute: A.role, status: -25200, skip: 1)]
        fake.deadline = fake.time + 0.11
        let outcome = run(fake, .pressOverview)
        let role = L(depth: 3, indexPath: [9, 0, 0], visited: 12, role: L.unreadable, subrole: L.absent, parentRole: "AXRow",
                     attributeNames: cell.attributeNames, identifierListed: false)
        check(refusal(outcome.error)?.rawValue == -25200
              && refusal(outcome.error)?.context == context(.copyAttributeValue, A.identifier, role, 1),
              "a failing AXRole read is recorded unreadable; the refusal keeps the identifier's -25200")
        check(admitted(outcome.error, .pressOverview), "an unreadable role is still a valid reply")
        let names = nested()
        names.faults = [.init(node: 22, attribute: A.identifier, status: -25201), .init(node: 21, attribute: A.role, status: -25204, skip: 1)]
        names.nameStatus[22] = -25204
        let blind = L(depth: 3, indexPath: [9, 0, 0], visited: 12, role: "AXCell", subrole: L.absent, parentRole: L.unreadable,
                      attributeNames: [], identifierListed: false)
        let unnamed = run(names, .pressOverview)
        check(refusal(unnamed.error)?.rawValue == -25201
              && refusal(unnamed.error)?.context == context(.copyAttributeValue, A.identifier, blind, 1) && unnamed.restarts == 0,
              "unreadable names and parent role leave empty names, unreadable parent and the terminal status")
        let text = nested()
        text.graph.nodes[22]!.strings[A.role] = "private UI text"
        text.graph.nodes[22]!.strings["AXSubrole"] = "Amber, 4 vCPU"
        text.faults = [.init(node: 22, attribute: A.identifier, status: -25201)]
        let hidden = refusal(run(text, .pressOverview).error)?.context?.locus
        check(hidden?.role == L.unreadable && hidden?.subrole == L.unreadable, "a role or subrole that is no AX constant is never recorded")
    }

    static func distinctLociCountIndexPaths() {
        let fake = nested()
        fake.deadline = fake.time + 0.11
        // The lookup reads 21 before 22: pass 1 fails at 22, every later pass at 21.
        fake.faults = [.init(node: 22, attribute: A.identifier, status: -25200, passes: 1...1),
                       .init(node: 21, attribute: A.identifier, status: -25202, passes: 2...Int.max)]
        let outcome = run(fake, .pressOverview)
        let row = L(depth: 2, indexPath: [9, 0], visited: 12, role: "AXRow", subrole: "AXOutlineRow", parentRole: "AXGroup",
                    attributeNames: [], identifierListed: false)
        check(refusal(outcome.error)?.rawValue == -25202 && refusal(outcome.error)?.context == context(.copyAttributeValue, A.identifier, row, 2)
              && outcome.restarts == 4, "two different failing elements across the restarts give distinctLoci 2 and the last locus")
        let alternating = nested()
        alternating.deadline = alternating.time + 0.11
        alternating.faults = [.init(node: 22, attribute: A.identifier, status: -25200, passes: 1...1),
                              .init(node: 22, attribute: A.identifier, status: -25200, passes: 3...3),
                              .init(node: 21, attribute: A.identifier, status: -25200, passes: 2...2),
                              .init(node: 21, attribute: A.identifier, status: -25200, passes: 4...Int.max)]
        check(refusal(run(alternating, .pressOverview).error)?.context?.distinctLoci == 2, "the count is of index paths, not of failures")
        let once = nested()
        once.faults = [.init(node: 22, attribute: A.identifier, status: -25200, passes: 1...1)]
        let recovered = run(once, .pressOverview)
        check(recovered.result?.outcome == .performed && once.presses == 1 && recovered.restarts == 1,
              "a transient failure that clears still presses once")
    }

    static func main() {
        persistentNestedIdentifier()
        unreadableFieldsStayRefusedWithTheFailingStatus()
        distinctLociCountIndexPaths()
        lookupWalkAndSheetLoci()
        failuresOutsideThePassHaveNoLocus()
        theRecorderChangesNothingElse()
        failurePathReadsOnlyRolesAndNames()
        bounds()
        productionNames()
        productionWiring()
        guard failed.isEmpty else {
            FileHandle.standardError.write(Data("FAIL: \(failed.count) of \(passed + failed.count) AX locus checks\n".utf8))
            exit(1)
        }
        print("PASS: \(passed) app UI driver AX locus checks; injected graphs, names and clocks, no GUI or permission query")
    }
}
