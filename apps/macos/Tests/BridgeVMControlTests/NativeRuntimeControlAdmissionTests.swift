import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeRuntimeControlAdmissionTests: XCTestCase {
    private typealias Fixture = NativeRuntimeControlTestSupport

    @MainActor func testExpiredMainActorAdmissionDoesNotTouchOwnerOrModel() {
        var ownerChecks = 0, modelReads = 0
        let router = NativeRuntimeAppControlRouter(appInstanceID: Fixture.app, library: Fixture.library,
            validateOwner: { ownerChecks += 1 }, retainedModel: { modelReads += 1; return nil })
        XCTAssertThrowsError(try router.handle(Fixture.request(), context: .init(deadline: NativeRuntimeTransport.now - 1)))
        XCTAssertEqual(ownerChecks, 0)
        XCTAssertEqual(modelReads, 0)
    }

    @MainActor func testOwnerFailureAndDeadlineSpentCheckingOwnerCannotMutate() {
        var modelReads = 0
        let failed = NativeRuntimeAppControlRouter(appInstanceID: Fixture.app, library: Fixture.library,
            validateOwner: { throw NativeRuntimeError.libraryChanged }, retainedModel: { modelReads += 1; return nil })
        XCTAssertThrowsError(try failed.handle(Fixture.request(), context: .init(deadline: NativeRuntimeTransport.now + 1)))
        let spent = NativeRuntimeAppControlRouter(appInstanceID: Fixture.app, library: Fixture.library,
            validateOwner: { Thread.sleep(forTimeInterval: 0.015) }, retainedModel: { modelReads += 1; return nil })
        XCTAssertThrowsError(try spent.handle(Fixture.request(), context: .init(deadline: NativeRuntimeTransport.now + 0.005)))
        XCTAssertEqual(modelReads, 0)
    }

    @MainActor func testCancelledHandlerDoesNotAdmitAfterItsActorHop() async {
        var ownerChecks = 0, modelReads = 0
        let router = NativeRuntimeAppControlRouter(appInstanceID: Fixture.app, library: Fixture.library,
            validateOwner: { ownerChecks += 1 }, retainedModel: { modelReads += 1; return nil })
        let task = Task { @MainActor in
            try router.handle(Fixture.request(), context: .init(deadline: NativeRuntimeTransport.now + 1))
        }
        task.cancel()
        do { _ = try await task.value; XCTFail("cancelled request admitted") } catch {}
        XCTAssertEqual(ownerChecks, 0)
        XCTAssertEqual(modelReads, 0)
    }
}
