import Foundation

/// Restarts one request's read-only AX pass after a transient AX status.
///
/// The pass is the window lookup, the walk and every read that locates or
/// observes an element. It starts again from the window lookup, after a yield
/// of at least `pause`, until the request's existing deadline. A mutation and
/// the readback after it never run inside a pass, so neither is replayed. Any
/// other AX status and every non-AX refusal stay terminal at once. When no
/// further restart fits before the deadline, the request is refused with the
/// last transient AX status and its call context, never a bare deadline.
final class AppUIDriverAXRestart {
    /// kAXErrorFailure, kAXErrorInvalidUIElement and kAXErrorCannotComplete.
    static let transient: Set<Int32> = [-25200, -25202, -25204]
    /// The least yield before each restart: a 5 s phase admits at most 200.
    static let pause = 0.025
    let deadline: Double
    let now: () -> Double
    let cancelled: () throws -> Bool
    private let yield: (Double) -> Void
    private(set) var count = 0

    init(deadline: Double, now: @escaping () -> Double, cancelled: @escaping () throws -> Bool,
         yield: @escaping (Double) -> Void) {
        self.deadline = deadline; self.now = now; self.cancelled = cancelled; self.yield = yield
    }

    /// The injected AX fixtures' default: the first transient status stays terminal.
    static func never() -> AppUIDriverAXRestart {
        AppUIDriverAXRestart(deadline: -.infinity, now: { 0 }, cancelled: { false }, yield: { _ in })
    }

    /// The driver's clock and main run loop. A run loop with no source to run
    /// returns at once, so the rest of that yield is slept instead.
    static func live(deadline: Double, cancelled: @escaping () throws -> Bool) -> AppUIDriverAXRestart {
        AppUIDriverAXRestart(deadline: deadline, now: { ProcessInfo.processInfo.systemUptime }, cancelled: cancelled) {
            let until = Date(timeIntervalSinceNow: $0)
            if !RunLoop.main.run(mode: .default, before: until) { Thread.sleep(until: until) }
        }
    }

    static func isTransient(_ error: AppUIDriverAXError) -> Bool {
        error.failure == .axFailure && error.rawValue.map(transient.contains) == true
    }

    func run<T>(_ pass: () throws -> T) throws -> T {
        var last: AppUIDriverAXError?
        while true {
            do { return try pass() }
            catch let error as AppUIDriverAXError where Self.isTransient(error) { last = error }
            // A restarted pass that reaches the deadline is refused with the status that restarted it.
            catch AppUIDriverFailure.deadlineExceeded where last != nil {}
            guard let error = last else { throw AppUIDriverFailure.internalFailure }
            try wait(refusing: error)
            count += 1
        }
    }

    private func wait(refusing error: AppUIDriverAXError) throws {
        let until = now() + Self.pause
        guard until < deadline else { throw error }
        while true {
            guard try !cancelled() else { throw AppUIDriverFailure.cancelled }
            let remaining = until - now()
            guard remaining > 0 else { return }
            yield(remaining)
        }
    }
}
