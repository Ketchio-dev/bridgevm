import Foundation

/// Retains the existing event task without retaining its controller through callbacks.
struct HvfOwnedRuntimeEventObserver: Sendable {
    private let task: Task<Void, Never>

    @MainActor
    init(events: AsyncStream<HvfOwnedRuntimeChannelEvent>, process: Process,
         receive: @escaping @MainActor @Sendable (HvfOwnedRuntimeChannelEvent) -> Void,
         terminated: @escaping @MainActor @Sendable () -> Void) {
        task = Task {
            for await event in events { receive(event) }
        }
        process.terminationHandler = { _ in Task { @MainActor in terminated() } }
    }

    func cancel() { task.cancel() }
}
