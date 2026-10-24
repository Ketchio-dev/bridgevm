import Foundation

/// The injected AX graph with scripted AX statuses and a fake clock. A fault
/// fires on a matching read (one node, or any node when `node` is nil) for
/// `remaining` reads and then stops; `Int.max` keeps it persistent. Like the
/// production wrapper, every read and mutation first refuses at the deadline.
/// Time advances only by `readCost` per read and by a restart's yields.
final class AppUIDriverAXTransientAccess: AppUIDriverAXAccess {
    struct Fault {
        let node: Int?, attribute: String, status: Int32
        var remaining = 1
    }
    let graph = AppUIDriverAXFixture()
    var faults: [Fault] = []
    var pressStatus: Int32?, searchStatus: Int32?
    var time = 100.0, readCost = 0.0, deadline = 105.0
    /// Cancellation checks answered false before the job's marker appears.
    var cancelAfter = Int.max
    private(set) var passes = 0, cancellationChecks = 0, pressAttempts = 0, searchAttempts = 0
    private(set) var yields: [Double] = []

    func restart() -> AppUIDriverAXRestart {
        AppUIDriverAXRestart(deadline: deadline, now: { self.time }, cancelled: {
            self.cancellationChecks += 1
            return self.cancellationChecks > self.cancelAfter
        }, yield: { self.yields.append($0); self.time += $0 })
    }

    /// Each pass starts at the application's window list.
    func application() -> Int { passes += 1; return graph.application() }
    func related(_ node: Int, _ attribute: String) throws -> [Int] {
        try read(node, attribute, .copyAttributeValues)
        return try graph.related(node, attribute)
    }
    func string(_ node: Int, _ attribute: String) throws -> String? {
        try read(node, attribute, .copyAttributeValue)
        return try graph.string(node, attribute)
    }
    func enabled(_ node: Int) throws -> Bool {
        try read(node, "AXEnabled", .copyAttributeValue)
        return try graph.enabled(node)
    }
    func press(_ node: Int) throws {
        guard time < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        pressAttempts += 1
        if let status = pressStatus { throw Self.error(status, .performAction, nil) }
        try graph.press(node)
    }
    func setSearch(_ node: Int) throws {
        guard time < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        searchAttempts += 1
        if let status = searchStatus { throw Self.error(status, .setAttributeValue, "AXValue") }
        try graph.setSearch(node)
    }
    func hash(_ node: Int) -> UInt { graph.hash(node) }
    func same(_ lhs: Int, _ rhs: Int) -> Bool { graph.same(lhs, rhs) }

    static func error(_ status: Int32, _ api: AppUIDriverAXContext.API, _ attribute: String?) -> AppUIDriverAXError {
        AppUIDriverAXError(failure: .axFailure, rawValue: status,
                           context: .init(api: api, attribute: .init(attribute), scope: .element))
    }

    private func read(_ node: Int, _ attribute: String, _ api: AppUIDriverAXContext.API) throws {
        guard time < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        time += readCost
        guard let index = faults.firstIndex(where: {
            ($0.node == nil || $0.node == node) && $0.attribute == attribute && $0.remaining > 0
        }) else { return }
        if faults[index].remaining != .max { faults[index].remaining -= 1 }
        throw Self.error(faults[index].status, api, attribute)
    }
}
