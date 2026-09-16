import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeRuntimeCodecTests: XCTestCase {
    private func request(saved: NativeRuntimeSavedConfiguration = .init(state: .missing, digest: nil)) -> NativeRuntimeRequest {
        NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
            requestID: UUID().uuidString,
            library: .init(canonicalPath: "/private/tmp/library", device: 1, inode: 2, uid: 501),
            vmID: "개발-vm", savedConfiguration: saved)
    }

    func testCanonicalRoundTripRefusesUnknownDuplicateAndNullFields() throws {
        let input = request()
        let bytes = try NativeRuntimeCodec.encode(input)
        XCTAssertEqual(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: bytes, limit: 8192), input)
        let text = String(decoding: bytes, as: UTF8.self)
        let variants = [" " + text, text.replacingOccurrences(of: "{", with: "{\"unknown\":0,", range: text.startIndex..<text.index(after: text.startIndex)),
            text.replacingOccurrences(of: "\"operation\":\"status\"", with: "\"operation\":\"status\",\"operation\":\"status\""),
            text.replacingOccurrences(of: "\"state\":\"missing\"", with: "\"digest\":null,\"state\":\"missing\"")]
        for value in variants {
            XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: Data(value.utf8), limit: 8192))
        }
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: bytes, limit: bytes.count - 1))
    }

    func testSavedConfigurationMustBindStateAndDigest() throws {
        try NativeRuntimeCodec.validate(request())
        try NativeRuntimeCodec.validate(request(saved: .init(state: .present, digest: String(repeating: "a", count: 64))))
        for saved in [NativeRuntimeSavedConfiguration(state: .present, digest: nil),
                      .init(state: .unreadable, digest: String(repeating: "a", count: 64)),
                      .init(state: .present, digest: "not-a-digest")] {
            XCTAssertThrowsError(try NativeRuntimeCodec.validate(request(saved: saved)))
        }
    }

    func testReplyBindsRequestAndRejectsInventedProcessAndConfigurationClaims() throws {
        let input = request()
        let response = NativeRuntimeResponse(schema: NativeRuntimeCodec.responseSchema, requestID: input.requestID,
            library: input.library, vmID: input.vmID, appInstanceID: UUID().uuidString,
            observedAt: 1_789_500_000, scope: NativeRuntimeCodec.scope, sessions: [])
        try NativeRuntimeCodec.validate(response, for: input)
        XCTAssertThrowsError(try NativeRuntimeCodec.validate(response, for: request()))
        let badClaims: [NativeRuntimeSessionObservation] = [
            .init(ownership: .owned, connectionState: "booting", acceptedConfigurationDigest: nil,
                  configurationMatch: .unknown, ownedProcess: nil, lastOwnedExit: nil),
            .init(ownership: .notObserved, connectionState: "stopped", acceptedConfigurationDigest: nil,
                  configurationMatch: .unknown, ownedProcess: nil, lastOwnedExit: nil),
            .init(ownership: .notObserved, connectionState: nil, acceptedConfigurationDigest: nil,
                  configurationMatch: .same, ownedProcess: nil, lastOwnedExit: nil)
        ]
        for claim in badClaims {
            XCTAssertThrowsError(try NativeRuntimeCodec.validate(claim, saved: input.savedConfiguration))
        }
    }
}
