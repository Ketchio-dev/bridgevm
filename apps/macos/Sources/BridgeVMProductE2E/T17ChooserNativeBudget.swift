import Foundation
import ApplicationServices

/// Cooperative admission only: a synchronous call already admitted cannot be preempted.
struct T17ChooserNativeBudget {
    enum Operation: String {
        case buttonLookup = "selection-button-lookup", enabled = "selection-enabled"
        case attributeNames = "ax-attribute-names", attributeValue = "ax-attribute-value"
        case relationship = "ax-relationship", snapshotAttempt = "snapshot-attempt"
        case retryPause = "snapshot-retry-pause", activation, keyboard = "keyboard-pair"
        case pathWrite = "path-write", selectionPress = "selection-press", raise = "panel-raise"
    }
    enum Boundary: String { case before, returned }
    let deadline: TimeInterval
    let now: () -> TimeInterval

    func check(_ operation: Operation, boundary: Boundary = .before, axResult: Int32? = nil) throws {
        let time = now()
        guard time.isFinite, deadline.isFinite, time < deadline else {
            let result = axResult.map { "; ax_error=\($0)" } ?? ""
            throw T17FileChooser.failure("native chooser deadline exhausted; operation=\(operation.rawValue); boundary=\(boundary.rawValue)" + result)
        }
    }
    func read<Value>(_ operation: Operation, _ read: () throws -> Value) throws -> Value {
        try T17ChooserNativeRead.perform(budget: self, operation: operation, read: read)
    }
    static func isDeadlineFailure(_ blocker: T17Blocker) -> Bool {
        blocker.code == "input-selection-failed" && blocker.detail.hasPrefix("native chooser deadline exhausted;")
    }
    func remaining() throws -> TimeInterval {
        let time = now()
        guard time.isFinite, deadline.isFinite, time < deadline else {
            throw T17FileChooser.failure("native chooser deadline exhausted; operation=activation; boundary=before")
        }
        return deadline - time
    }
    func input(_ operation: Operation, _ input: () -> AXError) throws -> AXError {
        try check(operation)
        let result = input()
        try check(operation, boundary: .returned, axResult: result.rawValue)
        return result
    }
    func snapshot<Node, Value>(root: () -> Node, nodes: (Node) throws -> [Node],
                               project: ([Node]) throws -> Value,
                               pause: () -> Void = { Thread.sleep(forTimeInterval: 0.2) }) throws -> Value {
        try read(.snapshotAttempt) {
            try T17FileChooserSnapshot.read(beforeRead: { try check(.snapshotAttempt) },
                pause: { try read(.retryPause, pause) }, root: root, nodes: nodes, project: project)
        }
    }
    var activationAdmitted: Bool { (try? check(.activation)) != nil }
}
