import Foundation

/// Holds the real writer before mutation and one actual old resource tuple before return.
/// No values are synthesized, and every wait has a finite bound.
final class ResourceDisplayStaleReadProbe {
    private let writeEntered = DispatchSemaphore(value: 0), allowWrite = DispatchSemaphore(value: 0)
    private let readCaptured = DispatchSemaphore(value: 0), allowRead = DispatchSemaphore(value: 0)
    private let readReturnBoundary = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var captured: [Int]?
    private var timedOut = false

    var snapshot: [Int]? { lock.withLock { captured } }
    var gateTimedOut: Bool { lock.withLock { timedOut } }
    func releaseWrite() { allowWrite.signal() }
    func releaseRead() { allowRead.signal() }
    func releaseAll() { releaseWrite(); releaseRead() }
    @MainActor func waitForWriteEntry() async -> Bool { await wait(writeEntered) }
    @MainActor func waitForReadCapture() async -> Bool { await wait(readCaptured) }
    @MainActor func waitForReadReturnBoundary() async -> Bool { await wait(readReturnBoundary) }

    func beforeWrite() {
        writeEntered.signal()
        if allowWrite.wait(timeout: .now() + 5) == .timedOut { lock.withLock { timedOut = true } }
    }

    func holdFirstRead(_ value: (memMiB: Int, cpu: Int)) -> (memMiB: Int, cpu: Int) {
        let shouldHold = lock.withLock {
            guard captured == nil else { return false }
            captured = [value.memMiB, value.cpu]
            return true
        }
        guard shouldHold else { return value }
        readCaptured.signal()
        if allowRead.wait(timeout: .now() + 5) == .timedOut { lock.withLock { timedOut = true } }
        // This marks the backend return boundary, NOT completion of refresh's MainActor.run.
        defer { readReturnBoundary.signal() }
        return value
    }

    @MainActor private func wait(_ semaphore: DispatchSemaphore) async -> Bool {
        await waitOffCooperativePool(semaphore, seconds: 3)
    }
}
