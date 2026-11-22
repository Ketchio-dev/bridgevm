import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeRuntimeRequestRoutingTests: XCTestCase {
    private typealias F = NativeRuntimeStartTestSupport

    /// Decoding ignores unknown fields, so a start request that also carries `savedConfiguration` decodes as a
    /// status request too. Routing follows `schema`, never the first type that happens to decode.
    func testStartWithStatusShapedExtraFieldStillReachesTheStartHandler() async throws {
        let request = F.request()
        let router = NativeRuntimeRequestRouter(library: F.library, status: { _ in
            XCTFail("start request routed to status"); throw NativeRuntimeError.invalidMessage
        }, control: { _, _ in
            XCTFail("start request routed to stop"); throw NativeRuntimeError.invalidMessage
        }, start: { received, _ in
            XCTAssertEqual(received, request)
            return F.response(received, refusal: .busy)
        })
        var text = String(decoding: try NativeRuntimeCodec.encode(request), as: UTF8.self)
        text.removeLast()
        text += ",\"savedConfiguration\":{\"state\":\"missing\"},\"addedLater\":true}"
        XCTAssertNoThrow(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: Data(text.utf8), limit: 8192))
        let reply = try await router.reply(to: Data(text.utf8), context: .init(deadline: NativeRuntimeTransport.now + 1))
        let response = try NativeRuntimeCodec.decode(NativeRuntimeStartResponse.self, from: reply,
                                                     limit: NativeRuntimeCodec.maximumResponseBytes)
        XCTAssertEqual(response.refusal, .busy)
    }

    func testUnknownSchemaIsRefused() async throws {
        let router = NativeRuntimeRequestRouter(library: F.library, status: { _ in
            XCTFail("unknown schema routed to status"); throw NativeRuntimeError.invalidMessage
        })
        var text = String(decoding: try NativeRuntimeCodec.encode(F.request()), as: UTF8.self)
        text = text.replacingOccurrences(of: NativeRuntimeStartCodec.requestSchema, with: "bridgevm.app-runtime-start-request.v9")
        do {
            _ = try await router.reply(to: Data(text.utf8), context: .init(deadline: NativeRuntimeTransport.now + 1))
            XCTFail("unknown schema accepted")
        } catch { XCTAssertEqual(error as? NativeRuntimeError, .invalidMessage) }
    }
}
