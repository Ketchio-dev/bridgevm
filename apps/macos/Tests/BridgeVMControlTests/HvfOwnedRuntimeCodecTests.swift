import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOwnedRuntimeCodecTests: XCTestCase {
    static var fixtures: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("tests/fixtures/owned-runtime-v1")
    }
    static func data(_ name: String) throws -> Data {
        try Data(contentsOf: fixtures.appendingPathComponent(name + ".json"))
    }
    static func event(_ name: String) throws -> HvfOwnedRuntimeEvent {
        try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeEvent.self, payload: data(name))
    }

    func testEverySharedGoldenPayloadIsByteExactIncludingExplicitNulls() throws {
        for name in ["hello-no-key", "hello-synthetic-key"] {
            let bytes = try Self.data(name)
            let value = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeHello.self, payload: bytes)
            XCTAssertEqual(try HvfOwnedRuntimeCodec.payload(value), bytes)
        }
        _ = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeStop.self, payload: Self.data("stop"))
        for name in ["ready", "stop-ack", "swtpm-started", "helper-started", "helper-reaped",
                     "swtpm-reaped", "cleanup-unconfirmed", "complete", "complete-not-admitted"] {
            let value = try Self.event(name)
            try HvfOwnedRuntimeEventValidation.validate(value)
            XCTAssertEqual(try HvfOwnedRuntimeCodec.payload(value), try Self.data(name))
        }
    }

    func testDuplicateUnknownOmittedNullBooleanAndNoncanonicalBytesFail() throws {
        let source = String(decoding: try Self.data("ready"), as: UTF8.self)
        for invalid in [
            source.replacingOccurrences(of: "\"schemaVersion\":1", with: "\"schemaVersion\":1,\"schemaVersion\":1"),
            source.replacingOccurrences(of: "\"schemaVersion\":1", with: "\"schemaVersion\":true"),
            source.replacingOccurrences(of: "\"schemaVersion\":1", with: "\"schemaVersion\":1.0"),
            source.replacingOccurrences(of: "\"child\":null,", with: ""),
            source.replacingOccurrences(of: "\"child\":null,", with: "\"alien\":null,\"child\":null,"),
            " " + source, source + "\n"
        ] {
            XCTAssertThrowsError(try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeEvent.self, payload: Data(invalid.utf8)))
        }
    }

    func testFragmentedFramesBoundsAndTwoFrameDrain() throws {
        let event = try Self.event("ready")
        let full = try HvfOwnedRuntimeCodec.frame(event)
        for split in 0..<full.count {
            var buffer = Data(full.prefix(split))
            XCTAssertNil(try HvfOwnedRuntimeCodec.takeFrame(from: &buffer))
            buffer.append(full.dropFirst(split))
            XCTAssertEqual(try HvfOwnedRuntimeCodec.takeFrame(from: &buffer), try Self.data("ready"))
            XCTAssertTrue(buffer.isEmpty)
        }
        var buffer = full + full
        XCTAssertNotNil(try HvfOwnedRuntimeCodec.takeFrame(from: &buffer))
        XCTAssertNotNil(try HvfOwnedRuntimeCodec.takeFrame(from: &buffer))
        XCTAssertTrue(buffer.isEmpty)
        for bytes: [UInt8] in [[0, 0, 0, 0], [0, 0, 32, 1], [255, 255, 255, 255]] {
            var invalid = Data(bytes)
            XCTAssertThrowsError(try HvfOwnedRuntimeCodec.takeFrame(from: &invalid))
        }
    }
}
