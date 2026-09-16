import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeStartAdmissionTests: XCTestCase {
    private typealias F = NativeRuntimeStartTestSupport

    func testExpiredAdmissionDoesNotReadOwnerOrModel() {
        var checks = 0, reads = 0
        let router = NativeRuntimeAppStartRouter(appInstanceID: F.app, library: F.library,
            validateOwner: { checks += 1 }, retainedModel: { reads += 1; return nil })
        XCTAssertThrowsError(try router.handle(F.request(), context: .init(deadline: NativeRuntimeTransport.now - 1)))
        XCTAssertEqual(checks, 0); XCTAssertEqual(reads, 0)
    }

    func testOwnerFailureAndTimeSpentCheckingCannotEnterModel() {
        var reads = 0
        for slow in [false, true] {
            let router = NativeRuntimeAppStartRouter(appInstanceID: F.app, library: F.library,
                validateOwner: { if slow { Thread.sleep(forTimeInterval: 0.015) } else { throw NativeRuntimeError.libraryChanged } },
                retainedModel: { reads += 1; return nil })
            XCTAssertThrowsError(try router.handle(F.request(), context: .init(deadline: NativeRuntimeTransport.now + 0.005)))
        }
        XCTAssertEqual(reads, 0)
    }

    func testUnknownStatusDoesNotCreateOrReadModel() throws {
        var reads = 0
        let router = NativeRuntimeAppStartRouter(appInstanceID: F.app, library: F.library,
            validateOwner: {}, retainedModel: { reads += 1; return nil })
        let result = try router.handle(F.request(.startStatus), context: .init(deadline: NativeRuntimeTransport.now + 1))
        XCTAssertEqual(result.refusal, .operationUnknown); XCTAssertEqual(reads, 0)
        let request = F.request()
        XCTAssertEqual(try router.handle(request, context: .init(deadline: NativeRuntimeTransport.now + 1)).refusal, .modelUnavailable)
        XCTAssertEqual(try router.handle(request, context: .init(deadline: NativeRuntimeTransport.now + 1)).refusal, .modelUnavailable)
        XCTAssertEqual(reads, 1)
    }

    func testCancelledTaskDoesNotAdmitAfterActorHop() async {
        var checks = 0, reads = 0
        let router = NativeRuntimeAppStartRouter(appInstanceID: F.app, library: F.library,
            validateOwner: { checks += 1 }, retainedModel: { reads += 1; return nil })
        let task = Task { @MainActor in try router.handle(F.request(), context: .init(deadline: NativeRuntimeTransport.now + 1)) }
        task.cancel()
        do { _ = try await task.value; XCTFail("cancelled start admitted") } catch {}
        XCTAssertEqual(checks, 0); XCTAssertEqual(reads, 0)
    }
}
