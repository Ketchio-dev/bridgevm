import Foundation

/// What never restarts: every other status, every non-AX refusal, cancellation,
/// a mutation and the readback after it; and the production yield itself.
extension AppUIDriverAXRestartContracts {
    /// (d) Every other AX status and every non-AX refusal stays terminal at once.
    static func otherStatusesStayTerminalWithoutRestart() {
        for status: Int32 in [-25211, -25201, -25203, -25206, -25208] {
            for (node, attribute, api) in [(8, A.identifier, AppUIDriverAXContext.API.copyAttributeValue),
                                           (1, A.children, .copyAttributeValues)] {
                let access = Access()
                access.faults = [.init(node: node, attribute: attribute, status: status)]
                let outcome = run(access, .pressOverview)
                check(refusal(outcome.error, status, api, attribute) && outcome.restarts == 0 && access.passes == 1
                      && access.yields.isEmpty && access.pressAttempts == 0, "status \(status) on \(attribute) is refused immediately")
            }
        }
        let missing = Access()
        missing.graph.nodes[1]!.related[A.children]!.removeAll { $0 == 6 }
        let absent = run(missing, .pressOverview)
        check(absent.error as? AppUIDriverFailure == .elementNotFound && absent.restarts == 0 && missing.passes == 1,
              "an absent overview button is refused at once")
        let twice = Access()
        twice.graph.add(20, role: "AXButton", identifier: "bridgevm.library.overview")
        twice.graph.nodes[1]!.related[A.children]!.append(20)
        let ambiguous = run(twice, .pressOverview)
        check(ambiguous.error as? AppUIDriverFailure == .ambiguousElement && ambiguous.restarts == 0 && twice.pressAttempts == 0,
              "a second overview button is refused at once")
        for after in [0, 1] {
            let access = Access()
            access.cancelAfter = after
            access.faults = [.init(node: 8, attribute: A.identifier, status: -25200)]
            let outcome = run(access, .pressOverview)
            check(outcome.error as? AppUIDriverFailure == .cancelled && outcome.restarts == 0 && access.passes == 1
                  && access.yields.count == after && access.pressAttempts == 0, "cancellation ends the restart pause (\(after) yields)")
        }
    }

    /// (e) A mutation and the readback after it are never replayed, whatever their status.
    static func mutationsAndReadbacksStaySingleShot() {
        for status: Int32 in [-25200, -25202, -25204] {
            let press = Access()
            press.pressStatus = status
            let refused = run(press, .pressOverview)
            check(refusal(refused.error, status, .performAction, "none") && press.pressAttempts == 1 && refused.restarts == 0,
                  "a press that returns \(status) is refused and never pressed twice")
            let after = Access()
            after.pressStatus = status
            after.faults = [.init(node: 8, attribute: A.identifier, status: -25200)]
            let late = run(after, .pressOverview)
            check(refusal(late.error, status, .performAction, "none") && after.pressAttempts == 1 && late.restarts == 1,
                  "a press that returns \(status) after a restart is still pressed once")
            let clear = Access()
            clear.pressStatus = status
            check(refusal(run(clear, .clearSearch).error, status, .performAction, "none") && clear.pressAttempts == 1,
                  "a clear-search press that returns \(status) is attempted once")
            let search = Access()
            search.searchStatus = status
            let set = run(search, .setSearchIndigo)
            check(refusal(set.error, status, .setAttributeValue, A.value) && search.searchAttempts == 1 && set.restarts == 0,
                  "a search value that returns \(status) is set once")
            let readback = Access()
            readback.faults = [.init(node: 9, attribute: A.value, status: status)]
            let read = run(readback, .setSearchIndigo)
            check(refusal(read.error, status, .copyAttributeValue, A.value) && readback.searchAttempts == 1 && read.restarts == 0,
                  "a readback after the search mutation that returns \(status) is not replayed")
        }
        let paced = Access()
        paced.faults = [.init(node: 9, attribute: "AXEnabled", status: -25202)]
        let set = run(paced, .setSearchIndigo)
        check(set.result?.values?.searchValue == "Indigo" && paced.searchAttempts == 1 && set.restarts == 1,
              "the reads before the search mutation restart; the mutation runs once")
    }

    /// The production clock and main run loop yield, without any AX access.
    static func liveRestartYieldsOnTheMainRunLoop() {
        func pass(_ restart: AppUIDriverAXRestart, failures: Int) -> (Error?, [Double]) {
            var starts: [Double] = []
            do {
                _ = try restart.run { () throws -> Bool in
                    starts.append(ProcessInfo.processInfo.systemUptime)
                    if starts.count <= failures { throw Access.error(-25200, .copyAttributeValue, A.identifier) }
                    return true
                }
                return (nil, starts)
            } catch { return (error, starts) }
        }
        for source in ["no run loop source", "a distant timer", "a timer that fires early"] {
            let timer = Timer(timeInterval: source == "a distant timer" ? 60 : 0.005, repeats: false) { _ in }
            if source != "no run loop source" { RunLoop.main.add(timer, forMode: .default) }
            let restart = AppUIDriverAXRestart.live(deadline: ProcessInfo.processInfo.systemUptime + 2, cancelled: { false })
            let (error, starts) = pass(restart, failures: 1)
            timer.invalidate()
            let gap = starts.count == 2 ? starts[1] - starts[0] : 0
            check(error == nil && restart.count == 1 && gap >= 0.020 && gap < 0.5,
                  "the live restart yields 20 ms or more and stays bounded with \(source) (\(gap) s)")
        }
        let cancelled = AppUIDriverAXRestart.live(deadline: ProcessInfo.processInfo.systemUptime + 2, cancelled: { true })
        let stopped = pass(cancelled, failures: 1)
        check(stopped.0 as? AppUIDriverFailure == .cancelled && stopped.1.count == 1 && cancelled.count == 0,
              "the live restart observes cancellation before it yields")
        let near = AppUIDriverAXRestart.live(deadline: ProcessInfo.processInfo.systemUptime + 0.01, cancelled: { false })
        let refused = pass(near, failures: 1)
        check(refusal(refused.0, -25200, .copyAttributeValue, A.identifier) && refused.1.count == 1 && near.count == 0,
              "the live restart never yields past the request deadline")
        let never = AppUIDriverAXRestart.never()
        check(refusal(pass(never, failures: 1).0, -25200, .copyAttributeValue, A.identifier) && never.count == 0,
              "the injected fixtures' default keeps the first transient status terminal")
        check(AppUIDriverAXRestart.pause >= 0.020 && AppUIDriverAXRestart.pause <= 0.050, "the restart pause is 20 to 50 ms")
        check(AppUIDriverAXRestart.transient == [-25200, -25202, -25204], "exactly three transient AX statuses")
    }
}
