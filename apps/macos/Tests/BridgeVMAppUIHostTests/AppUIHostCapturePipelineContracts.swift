#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

extension AppUIHostContractTests {
    func assertExpandedHostObservationContracts() async throws {
        try await assertAllIncompleteHostObservationContracts()
        try assertOwnedViewObservationContracts()
        try assertWindowServerCapturePolicy()
        try await assertAwaitedCaptureRefusesIncompleteRows()
    }

    func assertAwaitedCaptureRefusesIncompleteRows() async throws {
        for cancellation in [false, true] {
            let matrix = try matrixFixture()
            var resumed = false
            matrix.capture = { _ in
                XCTAssertEqual(matrix.persistedCounts, [0])
                XCTAssertEqual(matrix.observations, 0)
                await Task.yield()
                resumed = true
                XCTAssertEqual(try self.matrixRows(matrix).count, 0)
                if cancellation { withUnsafeCurrentTask { $0?.cancel() } }
                else { throw AppUIHostMatrixFixture.Failure.ownership }
            }
            let error = await Task { @MainActor in await self.matrixFailure(matrix) }.value
            XCTAssertTrue(resumed)
            if cancellation { XCTAssertTrue(error is CancellationError) }
            else { XCTAssertEqual(error as? AppUIHostMatrixFixture.Failure, .ownership) }
            XCTAssertEqual(matrix.captureAttempts, ["welcome-light-default"])
            XCTAssertEqual(matrix.presentations, ["welcome-light-default"])
            XCTAssertEqual(matrix.persistedCounts, [0])
            XCTAssertEqual(matrix.observations, 0)
            XCTAssertEqual(try matrixRows(matrix).count, 0)
            XCTAssertEqual(matrix.latchCount, 0)
        }
    }
}
#endif
