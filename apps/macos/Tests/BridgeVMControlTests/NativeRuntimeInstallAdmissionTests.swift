import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeInstallAdmissionTests: XCTestCase {
    private let app = "11111111-1111-1111-1111-111111111111"
    private let library = NativeRuntimeLibraryIdentity(canonicalPath: "/tmp/library", device: 1, inode: 2, uid: 3)

    private func request(app: String? = nil) -> NativeInstallControlRequest {
        .init(schema: NativeInstallControlCodec.requestSchema, operation: .installStatus,
            requestID: UUID().uuidString, library: library, vmID: "windows",
            appInstanceID: app ?? self.app,
            expectedSavedConfigurationDigest: String(repeating: "a", count: 64), operationID: nil)
    }

    func testExpiredAndCancelledAdmissionDoNotReadOwnerOrModel() async {
        var checks = 0, reads = 0
        let router = NativeRuntimeAppInstallRouter(appInstanceID: app, library: library,
            validateOwner: { checks += 1 }, retainedModel: { reads += 1; return nil })
        XCTAssertThrowsError(try router.handle(request(),
            context: .init(deadline: NativeRuntimeTransport.now - 1)))
        let task = Task { @MainActor in
            try router.handle(request(), context: .init(deadline: NativeRuntimeTransport.now + 1))
        }
        task.cancel()
        do { _ = try await task.value; XCTFail("cancelled install request admitted") } catch {}
        XCTAssertEqual(checks, 0); XCTAssertEqual(reads, 0)
    }

    func testOwnerFailureAndSpentDeadlineCannotReadModel() {
        var reads = 0
        let failed = NativeRuntimeAppInstallRouter(appInstanceID: app, library: library,
            validateOwner: { throw NativeRuntimeError.libraryChanged },
            retainedModel: { reads += 1; return nil })
        XCTAssertThrowsError(try failed.handle(request(),
            context: .init(deadline: NativeRuntimeTransport.now + 1)))
        let spent = NativeRuntimeAppInstallRouter(appInstanceID: app, library: library,
            validateOwner: { Thread.sleep(forTimeInterval: 0.015) },
            retainedModel: { reads += 1; return nil })
        XCTAssertThrowsError(try spent.handle(request(),
            context: .init(deadline: NativeRuntimeTransport.now + 0.005)))
        XCTAssertEqual(reads, 0)
    }

    func testOwnerMismatchAndMissingModelReturnBoundedRefusals() throws {
        var reads = 0
        let router = NativeRuntimeAppInstallRouter(appInstanceID: app, library: library,
            validateOwner: {}, retainedModel: { reads += 1; return nil })
        let changed = try router.handle(request(app: UUID().uuidString),
            context: .init(deadline: NativeRuntimeTransport.now + 1))
        XCTAssertEqual(changed.refusal, .ownerChanged); XCTAssertEqual(reads, 0)
        let missing = try router.handle(request(),
            context: .init(deadline: NativeRuntimeTransport.now + 1))
        XCTAssertEqual(missing.refusal, .modelUnavailable); XCTAssertEqual(reads, 1)
    }
}
