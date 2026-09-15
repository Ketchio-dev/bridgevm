#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

extension AppUIHostContractTests {
    func assertPresentationMatrixCancellation() async throws {
        let before = try matrixFixture()
        let beforeError = await Task { @MainActor in
            withUnsafeCurrentTask { $0?.cancel() }
            return await self.matrixFailure(before)
        }.value
        XCTAssertTrue(beforeError is CancellationError)
        XCTAssertTrue(before.persistAttempts.isEmpty)
        XCTAssertTrue(before.presentations.isEmpty)
        for duringCapture in [false, true] {
            let matrix = try matrixFixture()
            matrix.present = { variant in
                if variant.minimum && !duringCapture { withUnsafeCurrentTask { $0?.cancel() } }
            }
            matrix.capture = { variant in
                if variant.minimum && duringCapture { withUnsafeCurrentTask { $0?.cancel() } }
            }
            let error = await Task { @MainActor in await self.matrixFailure(matrix) }.value
            XCTAssertTrue(error is CancellationError)
            XCTAssertEqual(matrix.presentations.count, 2)
            XCTAssertEqual(matrix.captureAttempts.count, duringCapture ? 2 : 1)
            XCTAssertEqual(matrix.persistedCounts, [0, 1])
            XCTAssertEqual(matrix.observations, 1)
        }
        let marker = try matrixFixture()
        marker.admission = { try AppUIHost.checkCancellation(output: marker.output) }
        marker.present = { variant in
            if variant.minimum {
                try Data().write(to: marker.output.deletingLastPathComponent().appendingPathComponent("cancel.requested"))
                throw AppUIHostPresentationMismatch()
            }
        }
        let markerError = await matrixFailure(marker)
        XCTAssertTrue(markerError is AppUIHostError)
        XCTAssertEqual(marker.presentations.count, 2)
        XCTAssertEqual(marker.captureAttempts, ["welcome-light-default"])
        XCTAssertEqual(marker.persistedCounts, [0, 1])
        XCTAssertEqual(marker.latchCount, 0)
    }

    func assertPresentationMatrixPersistenceFailures() async throws {
        for afterMismatch in [false, true] {
            let matrix = try matrixFixture()
            var retained: Data?
            matrix.present = { variant in
                if variant.minimum { throw AppUIHostPresentationMismatch() }
            }
            matrix.beforePersist = { rows in
                if rows.count == (afterMismatch ? 2 : 0) {
                    if afterMismatch {
                        retained = try matrix.matrixData()
                        XCTAssertEqual(try self.object(matrix.output, "ui-observations.json")["failure"] as? String,
                                       String(describing: AppUIHostPresentationMismatch()))
                    }
                    try matrix.report.write([:], name: "missing-parent/matrix.json")
                }
            }
            let error = await matrixFailure(matrix)
            XCTAssertEqual((error as NSError?)?.domain, NSCocoaErrorDomain)
            XCTAssertEqual(matrix.presentations.count, afterMismatch ? 2 : 0)
            XCTAssertEqual(matrix.captureAttempts.count, afterMismatch ? 1 : 0)
            XCTAssertEqual(matrix.persistAttempts, afterMismatch ? [0, 1, 2] : [0])
            XCTAssertEqual(matrix.persistedCounts, afterMismatch ? [0, 1] : [])
            XCTAssertEqual(matrix.latchCount, afterMismatch ? 1 : 0)
            if afterMismatch {
                XCTAssertEqual(try matrix.matrixData(), retained)
                XCTAssertEqual(try matrixRows(matrix).count, 1)
            } else {
                XCTAssertFalse(FileManager.default.fileExists(atPath: matrix.output.appendingPathComponent("host-presentation-matrix.json").path))
            }
        }
    }

    func assertPresentationMatrixTypedCallbackFailures() async throws {
        for phase in ["admission", "capture", "latch", "persist"] {
            let matrix = try matrixFixture()
            matrix.admission = {
                if phase == "admission" { throw AppUIHostPresentationMismatch() }
            }
            matrix.present = { _ in
                if phase == "latch" { throw AppUIHostPresentationMismatch() }
            }
            matrix.capture = { _ in
                if phase == "capture" { throw AppUIHostPresentationMismatch() }
            }
            matrix.afterLatch = { throw AppUIHostPresentationMismatch() }
            matrix.beforePersist = { rows in
                if phase == "persist" && rows.count == 1 { throw AppUIHostPresentationMismatch() }
            }
            let error = await matrixFailure(matrix)
            XCTAssertTrue(error is AppUIHostPresentationMismatch, phase)
            XCTAssertEqual(matrix.presentations.count, phase == "admission" ? 0 : 1, phase)
            XCTAssertEqual(matrix.persistedCounts, [0], phase)
            XCTAssertEqual(matrix.latchCount, phase == "latch" ? 1 : 0, phase)
            XCTAssertEqual(try matrixRows(matrix).count, 0, phase)
        }
    }
}
#endif
