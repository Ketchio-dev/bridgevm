import ApplicationServices
import Foundation

/// The production wrapper's attribute-name message and the service's wiring around
/// AppUIDriverAX, with every system messaging closure injected: no app is messaged.
extension AppUIDriverAXLocusContracts {
    typealias F = AppUIDriverAXCallsFixture

    /// An injected attribute-name message and the number of times it was sent.
    final class Names {
        var sent = 0
        var calls: AppUIDriverAXNameCalls!
        init(_ value: CFArray?, status: AXError = .success) {
            calls = AppUIDriverAXNameCalls(copy: { _, output in self.sent += 1; output.pointee = value; return status })
        }
    }

    static func productionNames() {
        func read(_ f: F, _ names: Names, deadline: Double = 105) -> Result<[String], Error> {
            Result { try f.access(deadline: deadline).attributeNames(f.parent, calls: names.calls) }
        }
        func refused(_ result: Result<[String], Error>, _ code: AppUIDriverFailure) -> Bool {
            if case .failure(let error) = result { return error as? AppUIDriverFailure == code }
            return false
        }
        let f = F(), listed = Names(["AXRole", "AXIdentifier", "AXValue"] as CFArray)
        check((try? read(f, listed).get()) == ["AXRole", "AXIdentifier", "AXValue"] && listed.sent == 1,
              "every listed name is returned in order; the locus filters them")
        check(f.calls == [.getPID, .setMessagingTimeout] && f.attributes.isEmpty && f.timeouts == [0.25],
              "the names are prepared like every message and read no attribute value")
        let status = Names(nil, status: .failure)
        if case .failure(let error) = read(F(), status) {
            check((error as? AppUIDriverAXError)?.rawValue == -25200, "a failing names message is an AX error")
        } else { check(false, "a failing names message is an AX error") }
        check(refused(read(F(), Names(nil)), .invalidElement), "success without names is invalid")
        check(refused(read(F(), Names(unsafeBitCast("AXRole" as CFString, to: CFArray.self))), .invalidElement),
              "a value that is not an array is invalid")
        let other = F()
        check(refused(read(other, Names([other.child] as CFArray)), .invalidElement), "an entry that is not a string is invalid")
        check((try? read(F(), Names(Array(repeating: "AXRole", count: 1_024) as CFArray)).get())?.count == 1_024,
              "1,024 names are read")
        check(refused(read(F(), Names(Array(repeating: "AXRole", count: 1_025) as CFArray)), .protocolLimit), "1,025 names are refused")
        check(refused(read(F(), Names([String(repeating: "A", count: 1_025)] as CFArray)), .protocolLimit), "an oversized name is refused")
        let stranger = F(), unsent = Names(["AXRole"] as CFArray)
        stranger.observedPID = 99
        check(refused(read(stranger, unsent), .identityMismatch) && unsent.sent == 0, "another process's element is never asked")
        let expired = Names(["AXRole"] as CFArray)
        check(refused(read(F(), expired, deadline: 100), .deadlineExceeded) && expired.sent == 0, "no names message past the deadline")
        let cancelled = F(), stopped = Names(["AXRole"] as CFArray)
        cancelled.cancelled = true
        check(refused(read(cancelled, stopped), .cancelled) && stopped.sent == 0, "no names message after cancellation")
    }

    /// The service's wiring: AppUIDriverAXLocusAccess(AppUIDriverAX) under AppUIDriverAXOperations.
    static func productionWiring() {
        let _: (AppUIDriverAX) -> AppUIDriverAXLocusAccess<AppUIDriverAX> = { AppUIDriverAXLocusAccess($0) }
        let f = F(), names = Names(["AXIdentifier", "AXRole", "AXTitle", "private name"] as CFArray)
        f.array = [f.child]
        f.value = "AXWindow" as CFString
        f.fail = .copyAttributeValue
        f.error = .failure
        let access = AppUIDriverAXLocusAccess(f.access(), calls: names.calls)
        let outcome = Result { try AppUIDriverAXOperations(access: access).perform(.pressOverview) }
        guard case .failure(let error) = outcome, let refused = refusal(error) else {
            return check(false, "the window's failing identifier read is refused")
        }
        // The fixture answers every attribute with one value, so the role, subrole and parent role read alike.
        let window = L(depth: -1, indexPath: [0], visited: 0, role: "AXWindow", subrole: "AXWindow", parentRole: "AXWindow",
                       attributeNames: ["AXIdentifier", "AXRole", "AXTitle"], identifierListed: true)
        check(refused.rawValue == -25200 && refused.context == context(.copyAttributeValue, A.identifier, window, 1),
              "the production pass refuses with the window's exact locus")
        check(f.attributes == [A.windows, A.windows, A.identifier, A.role, "AXSubrole", A.role],
              "after the failing read only AXRole, AXSubrole and the parent's AXRole are read")
        check(names.sent == 1 && f.actions.isEmpty && f.assigned.isEmpty, "one names message and no mutation")
        check(admitted(refused, .pressOverview), "the production locus is a valid reply")
    }
}
