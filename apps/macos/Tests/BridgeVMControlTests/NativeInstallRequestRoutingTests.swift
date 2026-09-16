import XCTest
@testable import BridgeVMControl

final class NativeInstallRequestRoutingTests: XCTestCase {
    func testInstallFrameUsesDedicatedHandlerAndValidatesResponse() async throws {
        let library = NativeRuntimeLibraryIdentity(canonicalPath: "/tmp/library", device: 1, inode: 2, uid: 3)
        let app = "11111111-1111-1111-1111-111111111111"
        let request = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema,
            operation: .installStatus, requestID: UUID().uuidString, library: library, vmID: "windows",
            appInstanceID: app, expectedSavedConfigurationDigest: String(repeating: "a", count: 64),
            operationID: nil)
        let router = NativeRuntimeRequestRouter(library: library, status: { _ in
            throw NativeRuntimeError.invalidMessage
        }, install: { received, _ in
            XCTAssertEqual(received, request)
            return .init(schema: NativeInstallControlCodec.responseSchema,
                scope: NativeInstallControlCodec.scope, requestID: received.requestID,
                library: library, vmID: received.vmID, appInstanceID: app,
                expectedSavedConfigurationDigest: received.expectedSavedConfigurationDigest,
                requestedOperationID: nil, disposition: .refused, observation: nil,
                refusal: .targetUnavailable)
        })
        let bytes = try NativeRuntimeCodec.encode(request)
        let reply = try await router.reply(to: bytes,
            context: .init(deadline: NativeRuntimeTransport.now + 1))
        let decoded = try NativeRuntimeCodec.decode(NativeInstallControlResponse.self,
            from: reply, limit: NativeRuntimeCodec.maximumResponseBytes)
        XCTAssertEqual(decoded.refusal, .targetUnavailable)
    }

    func testInstallFrameWithoutHandlerFailsClosed() async throws {
        let library = NativeRuntimeLibraryIdentity(canonicalPath: "/tmp/library", device: 1, inode: 2, uid: 3)
        let request = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema,
            operation: .installStatus, requestID: UUID().uuidString, library: library, vmID: "windows",
            appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: String(repeating: "a", count: 64), operationID: nil)
        let router = NativeRuntimeRequestRouter(library: library, status: { _ in
            throw NativeRuntimeError.invalidMessage
        })
        do {
            _ = try await router.reply(to: NativeRuntimeCodec.encode(request),
                context: .init(deadline: NativeRuntimeTransport.now + 1))
            XCTFail("install request used another protocol handler")
        } catch { XCTAssertEqual(error as? NativeRuntimeError, .invalidMessage) }
    }
}
