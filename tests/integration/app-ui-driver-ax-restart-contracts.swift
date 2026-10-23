import Darwin
import Foundation

/// The d6 driver's read-only AX pass restarts from the window lookup after a
/// transient AX status, within the request deadline; a mutation and the readback
/// after it stay single-shot. Injected graph and clocks only: no GUI, no AX
/// messaging and no permission query. Every check runs; each failure is listed.
@main
enum AppUIDriverAXRestartContracts {
    typealias A = AppUIDriverAXAttribute
    typealias Access = AppUIDriverAXTransientAccess
    static var passed = 0, failed: [String] = []

    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) {
        if (try? condition()) == true { passed += 1; return }
        failed.append(name)
        FileHandle.standardError.write(Data("FAIL: \(name)\n".utf8))
    }

    static func run(_ access: Access, _ operation: AppUIDriverOperation)
        -> (result: AppUIDriverAXResult?, error: Error?, restarts: Int) {
        let restart = access.restart()
        do { return (try AppUIDriverAXOperations(access: access, restart: restart).perform(operation), nil, restart.count) }
        catch { return (nil, error, restart.count) }
    }

    /// Each yield is one restart pause (the fake clock subtracts, so compare within a nanosecond).
    static func paced(_ yields: [Double], _ count: Int) -> Bool {
        yields.count == count && yields.allSatisfy { abs($0 - AppUIDriverAXRestart.pause) < 1e-9 }
    }

    static func refusal(_ error: Error?, _ status: Int32, _ api: AppUIDriverAXContext.API, _ attribute: String) -> Bool {
        guard let error = error as? AppUIDriverAXError else { return false }
        return error.failure == .axFailure && error.rawValue == status
            && error.context == AppUIDriverAXContext(api: api, attribute: .init(attribute), scope: .element)
    }

    /// (a) and (b): each transient status, once, anywhere in the pass before the press.
    static func transientStatusesRestartTheWholePassOnce() {
        let sites: [(Int?, String, AppUIDriverAXContext.API)] = [(8, A.identifier, .copyAttributeValue),
            (nil, A.identifier, .copyAttributeValue), (1, A.children, .copyAttributeValues),
            (7, A.role, .copyAttributeValue), (6, "AXEnabled", .copyAttributeValue)]
        for status: Int32 in [-25200, -25202, -25204] {
            for (node, attribute, _) in sites {
                let access = Access()
                access.faults = [.init(node: node, attribute: attribute, status: status)]
                let outcome = run(access, .pressOverview)
                let site = "\(status) on \(attribute) of \(node.map(String.init) ?? "the first node read")"
                check(outcome.result?.outcome == .performed && outcome.result?.values == nil, "pressOverview performed after " + site)
                check(access.graph.presses == [6] && access.pressAttempts == 1, "exactly one overview press after " + site)
                check(outcome.restarts == 1 && access.passes == 2, "restart count 1 and a second pass from the window lookup after " + site)
                check(paced(access.yields, 1), "one bounded yield before the restart after " + site)
            }
        }
        let access = Access()
        access.faults = [.init(node: 8, attribute: A.identifier, status: -25200), .init(node: 1, attribute: A.children, status: -25202),
                         .init(node: 7, attribute: A.role, status: -25204)]
        let outcome = run(access, .pressOverview)
        check(outcome.result?.outcome == .performed && access.pressAttempts == 1 && outcome.restarts == 3 && access.passes == 4,
              "three successive transient statuses: three restarts, still one press")
    }

    /// (c) A status that persists until the injected deadline is refused like today's single-shot refusal.
    static func persistentStatusRefusesWithTheLastStatusAtTheDeadline() {
        let access = Access()
        access.deadline = access.time + 0.11
        access.faults = [.init(node: 8, attribute: A.identifier, status: -25200, remaining: .max)]
        let outcome = run(access, .pressOverview)
        check(refusal(outcome.error, -25200, .copyAttributeValue, A.identifier),
              "persistent -25200 is refused ax-failure -25200 with the copyAttributeValue/AXIdentifier/element context")
        check(!(outcome.error is AppUIDriverFailure), "a persistent transient status is not a bare deadline refusal")
        check(access.pressAttempts == 0 && access.graph.presses.isEmpty, "a persistent transient status never presses")
        check(outcome.restarts == 4 && access.passes == 5, "restarts only while a paced pass still fits before the deadline")
        check(paced(access.yields, 4) && access.time < access.deadline,
              "every restart is paced and none starts past the deadline")

        let last = Access()
        last.deadline = last.time + 0.11
        // The first pass stops at the window list (-25200); every later pass at the window's children (-25202).
        last.faults = [.init(node: 0, attribute: A.windows, status: -25200),
                       .init(node: 1, attribute: A.children, status: -25202, remaining: .max)]
        let changed = run(last, .pressOverview)
        check(refusal(changed.error, -25202, .copyAttributeValues, A.children) && changed.restarts == 4,
              "the refusal carries the last transient status and its context")

        let late = Access()
        late.readCost = 0.001
        late.deadline = late.time + 0.07
        late.faults = [.init(node: 3, attribute: A.identifier, status: -25200, remaining: .max)]
        let reached = run(late, .pressOverview)
        check(refusal(reached.error, -25200, .copyAttributeValue, A.identifier) && reached.restarts == 1 && late.passes == 2,
              "a restarted pass that reaches the deadline is refused with the transient status, not deadline-exceeded")
        let first = Access()
        first.readCost = 0.001
        first.deadline = first.time + 0.01
        let expired = run(first, .pressOverview)
        check(expired.error as? AppUIDriverFailure == .deadlineExceeded && expired.restarts == 0,
              "without a transient status the deadline stays deadline-exceeded")
        let short = Access()
        short.deadline = short.time + AppUIDriverAXRestart.pause * 0.8
        short.faults = [.init(node: 8, attribute: A.identifier, status: -25204)]
        let unpaced = run(short, .pressOverview)
        check(refusal(unpaced.error, -25204, .copyAttributeValue, A.identifier) && unpaced.restarts == 0 && short.yields.isEmpty,
              "no restart begins when its pause would end past the existing deadline")
        let window = Access()
        window.faults = [.init(node: nil, attribute: A.identifier, status: -25200, remaining: .max)]
        let bounded = run(window, .pressOverview)
        check(refusal(bounded.error, -25200, .copyAttributeValue, A.identifier) && (199...200).contains(bounded.restarts),
              "a five-second phase admits at most 200 paced restarts")
    }

    /// The observations right after the press and after the search restart too.
    static func observationsRestart() {
        let cards = Access()
        cards.faults = [.init(node: 7, attribute: A.description, status: -25200)]
        let rows = run(cards, .overviewCards)
        check(rows.result?.values?.amberVisible == true && rows.result?.values?.indigoVisible == true && rows.restarts == 1,
              "overviewCards observes both rows after a restart")
        let state = Access()
        state.faults = [.init(node: 9, attribute: A.value, status: -25202)]
        let search = run(state, .searchState)
        check(search.result?.values?.searchValue == "" && search.restarts == 1 && state.pressAttempts == 0,
              "searchState observes the search value after a restart")
    }

    static func main() {
        transientStatusesRestartTheWholePassOnce()
        persistentStatusRefusesWithTheLastStatusAtTheDeadline()
        otherStatusesStayTerminalWithoutRestart()
        mutationsAndReadbacksStaySingleShot()
        observationsRestart()
        liveRestartYieldsOnTheMainRunLoop()
        restartCountContracts()
        guard failed.isEmpty else {
            FileHandle.standardError.write(Data("FAIL: \(failed.count) of \(passed + failed.count) AX restart checks\n".utf8))
            exit(1)
        }
        print("PASS: \(passed) app UI driver AX restart checks; injected graph and clocks, no GUI or permission query")
    }
}
