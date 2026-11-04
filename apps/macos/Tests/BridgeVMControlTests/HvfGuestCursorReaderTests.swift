#if canImport(AppKit)
import Foundation
import XCTest
@testable import BridgeVMControl

/// display.cursor as the device writes it (crates/bridgevm-hvf/src/virtio_gpu/cursor_file.rs).
struct HvfGuestCursorFixture {
    var bytes = [UInt8](repeating: 0, count: 64 + 64 * 64 * 4)

    init(serial: UInt32 = 1, visible: Bool = true, shapeThisBoot: Bool = true, x: Int32 = 100, y: Int32 = 50,
         hot: (UInt32, UInt32) = (2, 4), sequence: UInt64 = 2, pixel: UInt8 = 0x5a) {
        for (offset, value): (Int, UInt32) in [(0, 0x4256_4352), (4, 1), (8, 64), (12, 64), (16, 256),
                                               (20, 0x3432_5241)] {
            set(value, at: offset)
        }
        set(serial, at: 32)
        set((visible ? 1 : 0) | (shapeThisBoot ? 2 : 0), at: 36)
        set(UInt32(bitPattern: x), at: 40)
        set(UInt32(bitPattern: y), at: 44)
        set(hot.0, at: 56)
        set(hot.1, at: 60)
        set(sequence, at: 24)
        for index in 64..<bytes.count { bytes[index] = pixel }
    }

    mutating func set<Value: FixedWidthInteger>(_ value: Value, at offset: Int) {
        withUnsafeBytes(of: value.littleEndian) { bytes.replaceSubrange(offset..<(offset + $0.count), with: $0) }
    }

    /// Writes the file, or rewrites it in place as the device does (same file, same length).
    func write(to url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else { return try Data(bytes).write(to: url) }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.write(contentsOf: Data(bytes))
    }
}

final class HvfGuestCursorReaderTests: XCTestCase {
    private func read(_ fixture: HvfGuestCursorFixture, length: Int? = nil, shown: UInt64 = .max,
                      serial: UInt32? = nil,
                      reader: HvfGuestCursorReader = HvfGuestCursorReader()) -> HvfGuestCursorReader.Outcome {
        fixture.bytes.withUnsafeBytes { buffer in
            reader.read(buffer.baseAddress!, length: length ?? buffer.count, shown: shown, serial: serial)
        }
    }

    func testAConsistentStateCarriesTheImageOnlyWhenTheSerialIsNew() {
        let fixture = HvfGuestCursorFixture(serial: 7, x: -3, y: 600, hot: (5, 9), sequence: 10)
        guard case let .state(state, sequence, image) = read(fixture) else { return XCTFail("no state") }
        XCTAssertEqual(state, HvfGuestCursorState(serial: 7, visible: true, shapeThisBoot: true, x: -3, y: 600,
                                                  hotX: 5, hotY: 9, width: 64, height: 64))
        XCTAssertEqual(sequence, 10)
        XCTAssertEqual(image, [UInt8](repeating: 0x5a, count: 64 * 64 * 4))
        guard case let .state(_, _, again) = read(fixture, serial: 7) else { return XCTFail("no state") }
        XCTAssertNil(again, "a move does not copy the image again")
        XCTAssertEqual(read(fixture, shown: 10), .unchanged)
    }

    func testTheWriterMidStateIsRetriedWithinTheBudgetThenLeftForTheNextTick() {
        var reader = HvfGuestCursorReader()
        reader.clock = { 0 }
        var loads = 0
        reader.loadSequence = { _ in loads += 1; return 3 }
        var pauses = 0
        reader.pause = { _, _ in pauses += 1 }
        XCTAssertEqual(read(HvfGuestCursorFixture(), reader: reader), .busy)
        XCTAssertEqual([loads, pauses], [3, 2])

        // The sequence moved during the copy, then settled: taken on the retry.
        let script: [UInt64] = [2, 4, 4, 4]
        loads = 0
        reader.loadSequence = { _ in loads += 1; return script[min(loads, script.count) - 1] }
        guard case let .state(_, sequence, _) = read(HvfGuestCursorFixture(), reader: reader) else {
            return XCTFail("no state")
        }
        XCTAssertEqual(sequence, 4)
    }

    /// The file is the app's input, not the guest's: nothing in it sizes a read beyond 64x64 and the mapping.
    func testAFileThisReaderMustNotTrustIsInvalid() {
        var cases: [(String, HvfGuestCursorFixture, Int?)] = []
        func with(_ label: String, length: Int? = nil, _ change: (inout HvfGuestCursorFixture) -> Void) {
            var fixture = HvfGuestCursorFixture()
            change(&fixture)
            cases.append((label, fixture, length))
        }
        with("magic") { $0.set(UInt32(0x4256_4642), at: 0) }
        with("version") { $0.set(UInt32(2), at: 4) }
        with("width over 64") { $0.set(UInt32(65), at: 8); $0.set(UInt32(260), at: 16) }
        with("zero height") { $0.set(UInt32(0), at: 12) }
        with("stride") { $0.set(UInt32(512), at: 16) }
        with("fourcc") { $0.set(UInt32(0x3432_5258), at: 20) }
        with("hot spot outside") { $0.set(UInt32(64), at: 56) }
        with("short mapping", length: 64 + 64 * 64 * 4 - 1) { _ in }
        with("header only", length: 63) { _ in }
        for (label, fixture, length) in cases {
            XCTAssertEqual(read(fixture, length: length), .invalid, label)
        }
    }
}
#endif
