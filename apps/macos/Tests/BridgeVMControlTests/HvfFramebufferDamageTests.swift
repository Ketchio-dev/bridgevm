#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// A 2D frame goes into the display's IOSurface behind the one shown, and only the pixels the writer says
/// changed (bytes 44..48 and 56..64, crates/bridgevm-hvf/src/virtio_gpu/fb_sink_header.rs) are copied,
/// when they can be trusted. Each frame here also changes pixel 0, which its damage leaves out: a reader
/// that copied the whole frame shows it.
@MainActor
final class HvfFramebufferDamageTests: XCTestCase {
    private static let gray: UInt32 = 0x0010_1010

    func testTrustedDamageCopiesOnlyItsPixelsWithTheFrontFramesIntoTheSurfaceBehind() throws {
        try withDisplay { view, publish in
            let first = try publish(Frame(Array(repeating: Self.gray, count: 8), sequence: 2))
            XCTAssertEqual(try Self.shown(view), first.pixels, "the first frame is copied whole")
            let second = try publish(first.changing([1: 0xb1, 0: 0xdead], sequence: 4, damage: [1, 0, 1, 1]))
            XCTAssertEqual(try Self.shown(view), second.pixels, "the surface behind held no frame: whole")
            let front = try Fixture.contents(view)
            let third = try publish(second.changing([6: 0xc6, 0: 0xbeef], sequence: 6, damage: [2, 1, 1, 1]))
            var expected = third.pixels
            expected[0] = Self.gray
            XCTAssertEqual(try Self.shown(view), expected,
                           "the surface behind held frame 2: it gets frame 4's damage and frame 6's, nothing else")
            XCTAssertFalse(try Fixture.contents(view) === front, "never written while shown")
        }
    }

    func testDamageThatCannotBeTrustedCopiesTheWholeFrame() throws {
        let cases: [(String, (Frame) -> Frame)] = [
            ("damage words of another frame", { $0.changing([0: 0xdead], sequence: 8, damage: [1, 0, 1, 1], tag: 6) }),
            ("a frame skipped", { $0.changing([0: 0xdead], sequence: 10, damage: [1, 0, 1, 1]) }),
            ("damage outside the frame", { $0.changing([0: 0xdead], sequence: 8, damage: [3, 0, 2, 1]) }),
            ("a new geometry", { Frame(Array($0.changing([0: 0xdead], sequence: 8).pixels.reversed()), width: 2,
                                       sequence: 8, damage: [1, 0, 1, 1]) }),
        ]
        for (name, next) in cases {
            try withDisplay { view, publish in
                let sixth = try publishTrustedFrames(publish)
                let frame = try publish(next(sixth))
                XCTAssertEqual(try Self.shown(view), frame.pixels, name)
            }
        }
    }

    func testAReplacedFileIsCopiedWholeEvenWhenItsDamageMatchesItsSequence() throws {
        try withDisplay { view, publish in
            let sixth = try publishTrustedFrames(publish)
            let path = try XCTUnwrap(view.framebufferPath)
            let replacement = sixth.changing([0: 0xdead], sequence: 8, damage: [1, 0, 1, 1])
            try replacement.data.write(to: URL(fileURLWithPath: path + ".new"))
            XCTAssertEqual(Darwin.rename(path + ".new", path), 0)
            view.refreshFrame(at: 10)
            view.refreshFrame(at: 11)
            XCTAssertEqual(try Self.shown(view), replacement.pixels)
        }
    }

    func testATornCopyLeavesTheSurfaceBehindToBeCopiedWhole() throws {
        try withDisplay { view, publish in
            let sixth = try publishTrustedFrames(publish)
            // The first attempt copies a frame whose damage cannot be trusted, and sees the writer publish
            // during the copy; the frame the retry finds has trusted damage.
            let torn = Frame(Array(repeating: 0xbad, count: 8), sequence: 8)
            let settled = sixth.changing([0: 0xdead, 1: 0xb2], sequence: 8, damage: [1, 0, 1, 1])
            let path = try XCTUnwrap(view.framebufferPath)
            try torn.write(path)
            var loads = 0
            view.frameReader.clock = { 0 }
            view.frameReader.loadSequence = { pointer in
                loads += 1
                return HvfFramebufferFrameReader.acquireSequence(pointer) + (loads == 2 ? 2 : 0)
            }
            view.frameReader.pause = { _, _ in
                do { try settled.write(path) } catch { XCTFail("the writer could not finish its frame: \(error)") }
            }
            view.refreshFrame(at: 10)
            XCTAssertEqual(loads, 4)
            XCTAssertEqual(try Self.shown(view), settled.pixels)
        }
    }

    func testTheCopiedRegionsAndOpaquePixels() throws {
        try withDisplay { view, publish in
            _ = try publishTrustedFrames(publish)
            XCTAssertEqual(view.surfaces?.copied, HvfFramebufferDamage(x: 1, y: 0, width: 2, height: 2))
            let raw = try Fixture.pixels(view)
            XCTAssertEqual(raw[3], 0xff, "the fourth byte, which the CGImage skipped, is opaque alpha")
            let contents = try XCTUnwrap(view.layer?.contents) as CFTypeRef
            XCTAssertEqual(CFGetTypeID(contents), IOSurfaceGetTypeID())
        }
        let damage = HvfFramebufferDamage(x: 2, y: 1, width: 1, height: 1)
        XCTAssertEqual(HvfFramebufferDamage(x: 0, y: 0, width: 0, height: 0).union(damage), damage)
        XCTAssertEqual(damage.union(HvfFramebufferDamage(x: 5, y: 5, width: 0, height: 3)), damage)
    }

    func testFramesAreShownAsImagesWhenNoSurfaceCanBeMade() throws {
        try withDisplay { view, publish in
            view.surfaces?.make = { _, _ in nil }
            let frame = Frame([Self.gray, 0xb1, 0, 0, 0, 0, 0, 0], sequence: 2)
            try frame.write(try XCTUnwrap(view.framebufferPath))
            XCTAssertEqual(view.refreshFrame(at: 0), .busy)
            XCTAssertNil(view.surfaces)
            XCTAssertNil(view.layer?.contents)
            XCTAssertEqual(try publish(frame).pixels, try Self.shown(view))
            let contents = try XCTUnwrap(view.layer?.contents) as CFTypeRef
            XCTAssertEqual(CFGetTypeID(contents), CGImage.typeID)
        }
    }

    // MARK: - Fixture

    private typealias Frame = HvfFramebufferDamageFrame
    private typealias Fixture = HvfFramebufferAvailabilityFixture

    /// Frames 2 and 4 copied whole, then frame 6 with its damage and frame 4's: 4x2 gray with pixel 1 at
    /// 0xb1 and pixel 6 at 0xc6.
    private func publishTrustedFrames(_ publish: (Frame) throws -> Frame) throws -> Frame {
        let first = try publish(Frame(Array(repeating: Self.gray, count: 8), sequence: 2))
        let second = try publish(first.changing([1: 0xb1], sequence: 4, damage: [1, 0, 1, 1]))
        return try publish(second.changing([6: 0xc6], sequence: 6, damage: [2, 1, 1, 1]))
    }

    /// The shown frame's pixels without their fourth byte.
    private static func shown(_ view: FBLayerView) throws -> [UInt32] {
        let bytes = [UInt8](try Fixture.pixels(view))
        return stride(from: 0, to: bytes.count, by: 4).map {
            UInt32(bytes[$0]) | UInt32(bytes[$0 + 1]) << 8 | UInt32(bytes[$0 + 2]) << 16
        }
    }

    /// A view over a BVFB file with room for 8 pixels; `publish` writes a frame in place and shows it.
    private func withDisplay(_ body: (FBLayerView, (Frame) throws -> Frame) throws -> Void) throws {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data(count: Frame.fileLength).write(to: root.appendingPathComponent("display.fb"))
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session)
        defer { view.teardown() }
        var tick = 0.0
        let path = root.appendingPathComponent("display.fb").path
        try body(view) { frame in
            try frame.write(path)
            tick += 1
            guard case .presented = view.refreshFrame(at: tick) else {
                XCTFail("frame \(frame.sequence) was not shown")
                return frame
            }
            return frame
        }
        withExtendedLifetime(session) {}
    }
}

/// One BVFB frame as a writer with damage words leaves it.
struct HvfFramebufferDamageFrame {
    static let fileLength = 64 + 8 * 4

    var pixels: [UInt32]
    var width = 4
    var sequence: UInt64
    /// x, y, width, height; nil writes no damage words (tag 0).
    var damage: [UInt16]?
    var tag: UInt32?

    init(_ pixels: [UInt32], width: Int = 4, sequence: UInt64, damage: [UInt16]? = nil, tag: UInt32? = nil) {
        (self.pixels, self.width, self.sequence, self.damage, self.tag) = (pixels, width, sequence, damage, tag)
    }

    /// The next frame: `changes` applied (index: pixel), with this damage.
    func changing(_ changes: [Int: UInt32], sequence: UInt64, damage: [UInt16]? = nil, tag: UInt32? = nil) -> Self {
        var next = Self(pixels, width: width, sequence: sequence, damage: damage, tag: tag)
        for (index, pixel) in changes { next.pixels[index] = pixel }
        return next
    }

    var data: Data {
        var data = Data(count: Self.fileLength)
        func put<Value: FixedWidthInteger>(_ value: Value, at offset: Int) {
            withUnsafeBytes(of: value.littleEndian) { data.replaceSubrange(offset ..< offset + $0.count, with: $0) }
        }
        for (index, value) in [0x42564642, 1, UInt32(width), UInt32(pixels.count / width), UInt32(width * 4),
                               0x34325258].enumerated() {
            put(UInt32(value), at: index * 4)
        }
        put(sequence, at: 24)
        if let damage {
            put(tag ?? UInt32(truncatingIfNeeded: sequence), at: 44)
            for (index, value) in damage.enumerated() { put(value, at: 56 + 2 * index) }
        }
        for (index, pixel) in pixels.enumerated() { put(pixel, at: 64 + 4 * index) }
        return data
    }

    /// Writes the frame over the file at `path` in place, so a view's mapping of it stays.
    func write(_ path: String) throws {
        let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        defer { try? handle.close() }
        try handle.write(contentsOf: data)
    }
}
#endif
