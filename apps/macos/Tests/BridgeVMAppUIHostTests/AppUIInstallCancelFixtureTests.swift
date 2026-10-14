#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class AppUIInstallCancelFixtureTests: XCTestCase {
    func testActualSessionCancellationAcknowledgesBeforeRetryAndPreservesFixture() async throws {
        try await withFixture { fixture, before in
            // Materialize the same selected model used by the install detail, without constructing a window.
            _ = fixture.library.model(for: fixture.config)
            let session = try await fixture.prepared()
            XCTAssertTrue(session === fixture.session)
            XCTAssertTrue(try fixture.matches(.idle))
            self.assertCounters(fixture, calls: 0, finished: 0, resets: 0, cancels: 0)
            try self.assertPreserved(fixture, before)

            let acknowledgment = try XCTUnwrap(session.start())
            let entered = try await self.waitForValidation(fixture)
            XCTAssertTrue(entered, "The actual detached validator must enter its finite gate")
            guard entered else { return }
            XCTAssertTrue(Thread.isMainThread, "The main actor progresses while validation is held")
            XCTAssertTrue(session.isRunning); XCTAssertTrue(session.canCancel)
            XCTAssertEqual(session.stage, .validating)
            XCTAssertTrue(try fixture.matches(.validating))
            XCTAssertNil(session.start(), "Duplicate start cannot reset the held attempt")
            self.assertCounters(fixture, calls: 1, finished: 0, resets: 1, cancels: 0)

            session.cancel()
            XCTAssertEqual(session.stage, .cancelling)
            XCTAssertTrue(session.isRunning); XCTAssertFalse(session.canCancel)
            XCTAssertTrue(try fixture.matches(.cancelling))
            XCTAssertNil(session.start(), "Retry remains unavailable until validation acknowledges cancellation")
            session.cancel()
            self.assertCounters(fixture, calls: 1, finished: 0, resets: 1, cancels: 1)
            try self.assertPreserved(fixture, before)

            fixture.gate.release()
            await acknowledgment.value
            XCTAssertEqual(session.stage, .cancelled)
            XCTAssertFalse(session.isRunning); XCTAssertFalse(session.canCancel)
            XCTAssertTrue(try fixture.matches(.cancelled))
            self.assertCounters(fixture, calls: 1, finished: 1, resets: 1, cancels: 1)
            try self.assertPreserved(fixture, before)

            let retry = try XCTUnwrap(session.start())
            await retry.value
            XCTAssertEqual(session.stage, .failed("합성 UI 검증 종료"))
            XCTAssertFalse(session.isRunning); XCTAssertFalse(session.canCancel)
            XCTAssertTrue(try fixture.matches(.retryFailed))
            self.assertCounters(fixture, calls: 2, finished: 2, resets: 2, cancels: 1)
            try self.assertPreserved(fixture, before)
        }
    }

    func testDiagnosticAbortSettlesHeldValidationWithoutLaunchingOrDeletingInputs() async throws {
        try await withFixture { fixture, before in
            _ = fixture.library.model(for: fixture.config)
            let session = try await fixture.prepared()
            let acknowledgment = try XCTUnwrap(session.start())
            let entered = try await self.waitForValidation(fixture)
            XCTAssertTrue(entered)
            guard entered else { return }
            XCTAssertEqual(fixture.counters.snapshot["validation_finished"], 0)

            let settled = await fixture.settle()
            XCTAssertTrue(settled, "Abort cancels the session and releases its finite validation gate")
            await acknowledgment.value
            XCTAssertEqual(session.stage, .cancelled)
            XCTAssertFalse(session.isRunning); XCTAssertFalse(session.canCancel)
            self.assertCounters(fixture, calls: 1, finished: 1, resets: 1, cancels: 1)
            try self.assertPreserved(fixture, before)

            let settledAgain = await fixture.settle()
            XCTAssertTrue(settledAgain, "Repeated settlement has no additional cancellation effect")
            self.assertCounters(fixture, calls: 1, finished: 1, resets: 1, cancels: 1)
            try self.assertPreserved(fixture, before)
        }
    }
}
#endif
