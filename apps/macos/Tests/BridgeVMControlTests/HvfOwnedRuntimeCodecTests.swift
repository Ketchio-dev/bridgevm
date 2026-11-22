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

    func testSharedGoldenPayloadsDecodeAndOwnerFramesMatchThem() throws {
        for name in ["hello-no-key", "hello-synthetic-key"] {
            let bytes = try Self.data(name)
            let value = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeHello.self, payload: bytes)
            XCTAssertEqual(try HvfOwnedRuntimeCodec.payload(value), bytes)
        }
        let stop = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeStop.self, payload: Self.data("stop"))
        XCTAssertEqual(try HvfOwnedRuntimeCodec.payload(stop), try Self.data("stop"))
        for name in ["ready", "stop-ack", "swtpm-started", "helper-started", "helper-reaped",
                     "swtpm-reaped", "cleanup-unconfirmed", "complete", "complete-not-admitted"] {
            try HvfOwnedRuntimeEventValidation.validate(Self.event(name))
        }
    }

    func testRunnerFormattingDifferencesAreAcceptedButWrongTypesAreNot() throws {
        let source = String(decoding: try Self.data("ready"), as: UTF8.self)
        let expected = try Self.event("ready")
        for tolerated in [
            source.replacingOccurrences(of: "\"child\":null,", with: ""),
            source.replacingOccurrences(of: "\"child\":null,", with: "\"alien\":{\"x\":1},\"child\":null,"),
            source.replacingOccurrences(of: "\"termMillis\":2000", with: "\"termMillis\":8000"),
            "{\"sequence\":1," + source.dropFirst().replacingOccurrences(of: "\"sequence\":1,", with: ""),
            " " + source + "\n"
        ] {
            let value = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeEvent.self, payload: Data(tolerated.utf8))
            XCTAssertEqual(value, expected)
            try HvfOwnedRuntimeEventValidation.validate(value)
        }
        for invalid in [source.replacingOccurrences(of: "\"schemaVersion\":1", with: "\"schemaVersion\":true"),
                        source.replacingOccurrences(of: "\"runnerPID\":1234,", with: ""), String(source.dropLast())] {
            XCTAssertThrowsError(try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeEvent.self, payload: Data(invalid.utf8)))
        }
    }

    func testFragmentedFramesBoundsAndTwoFrameDrain() throws {
        let stop = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeStop.self, payload: Self.data("stop"))
        let full = try HvfOwnedRuntimeCodec.frame(stop)
        for split in 0..<full.count {
            var buffer = Data(full.prefix(split))
            XCTAssertNil(try HvfOwnedRuntimeCodec.takeFrame(from: &buffer))
            buffer.append(full.dropFirst(split))
            XCTAssertEqual(try HvfOwnedRuntimeCodec.takeFrame(from: &buffer), try Self.data("stop"))
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
