import Foundation

/// The injected AX graph for the locus contracts: scripted AX statuses per pass,
/// scripted attribute names and a fake clock. Every read and attribute-name read
/// is logged with its pass and whether it followed that pass's failing read.
/// Like the production wrapper, every read first refuses at the deadline.
final class AppUIDriverAXLocusFake: AppUIDriverAXAccess {
    struct Fault {
        let node: Int?, attribute: String, status: Int32
        /// The passes in which it fires, once `skip` matching reads of that pass have succeeded.
        var passes = 1...Int.max, skip = 0
    }
    struct Read: Equatable { let pass: Int, node: Int, attribute: String, afterFailure: Bool }
    static let names = "(attribute names)"
    let graph = AppUIDriverAXFixture()
    var faults: [Fault] = [], names: [Int: [String]] = [:], nameStatus: [Int: Int32] = [:]
    var pressStatus: Int32?, searchStatus: Int32?
    var time = 100.0, deadline = 105.0
    private(set) var pass = 0, reads: [Read] = [], presses = 0, searches = 0
    private var matched: [Int: Int] = [:], failed = false

    func restart() -> AppUIDriverAXRestart {
        AppUIDriverAXRestart(deadline: deadline, now: { self.time }, cancelled: { false }, yield: { self.time += $0 })
    }
    /// Each pass starts at the application's window list.
    func application() -> Int { pass += 1; matched = [:]; failed = false; return graph.application() }
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
        presses += 1
        if let pressStatus { throw Self.error(pressStatus, .performAction, nil) }
        try graph.press(node)
    }
    func setSearch(_ node: Int) throws {
        searches += 1
        if let searchStatus { throw Self.error(searchStatus, .setAttributeValue, "AXValue") }
        try graph.setSearch(node)
    }
    func hash(_ node: Int) -> UInt { graph.hash(node) }
    func same(_ lhs: Int, _ rhs: Int) -> Bool { graph.same(lhs, rhs) }

    func attributeNames(_ node: Int) throws -> [String] {
        reads.append(Read(pass: pass, node: node, attribute: Self.names, afterFailure: failed))
        if let status = nameStatus[node] { throw Self.error(status, .copyAttributeValues, nil) }
        return names[node] ?? []
    }

    static func error(_ status: Int32, _ api: AppUIDriverAXContext.API, _ attribute: String?) -> AppUIDriverAXError {
        AppUIDriverAXError(failure: .axFailure, rawValue: status,
                           context: .init(api: api, attribute: .init(attribute), scope: .element))
    }

    private func read(_ node: Int, _ attribute: String, _ api: AppUIDriverAXContext.API) throws {
        guard time < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        reads.append(Read(pass: pass, node: node, attribute: attribute, afterFailure: failed))
        for (index, fault) in faults.enumerated() where fault.passes.contains(pass) && fault.attribute == attribute
            && (fault.node == nil || fault.node == node) {
            matched[index, default: 0] += 1
            if matched[index]! > fault.skip { failed = true; throw Self.error(fault.status, api, attribute) }
        }
    }
}
