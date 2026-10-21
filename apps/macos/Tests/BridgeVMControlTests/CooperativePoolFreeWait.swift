import Foundation

/// Waits for a test gate on a dedicated thread, then resumes the main-actor caller.
/// Hosted macOS runners have three CPUs, so the cooperative pool has three threads.
/// In the gated resource tests the product's reader and writer each block one, and
/// resuming the awaiting test needs the third (measured locally), so the waiter must
/// not block a pool thread itself. A GCD worker was measured not to start while the
/// pool was fully blocked; a plain thread starts at once.
@MainActor func waitOffCooperativePool(_ semaphore: DispatchSemaphore, seconds: Double) async -> Bool {
    await withCheckedContinuation { continuation in
        Thread {
            continuation.resume(returning: semaphore.wait(timeout: .now() + seconds) == .success)
        }.start()
    }
}
