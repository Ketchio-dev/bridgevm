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

    func testOwnerMismatchAndMissingModelReturnBoundedRefusals() throws {
        var reads = 0
        let router = NativeRuntimeAppInstallRouter(appInstanceID: app, library: library,
            validateOwner: {}, retainedModel: { reads += 1; return nil })
        let changed = try router.handle(request(app: UUID().uuidString))
        XCTAssertEqual(changed.refusal, .ownerChanged); XCTAssertEqual(reads, 0)
        let missing = try router.handle(request())
        XCTAssertEqual(missing.refusal, .modelUnavailable); XCTAssertEqual(reads, 1)
    }
}
