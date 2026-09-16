import Foundation
import XCTest

/// Uses the common XCTest/shim assertion surface and never blocks the main actor.
@MainActor
final class HvfOwnedRuntimeChannelTestWait {
    private var completedAt: TimeInterval?
    private let description: String

    init(_ description: String) { self.description = description }

    func complete() { completedAt = ProcessInfo.processInfo.systemUptime }

    func wait() async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 3
        while completedAt == nil, ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(completedAt.map { $0 <= deadline } ?? false,
                      "\(description) must complete within the original 3-second bound")
    }
}
