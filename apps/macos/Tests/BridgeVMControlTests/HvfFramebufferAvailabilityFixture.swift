import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
enum HvfFramebufferAvailabilityFixture {
    static func run(factory: HvfFramebufferImageFactory = .init(),
                    _ body: (HvfRuntimeKeyboardDraftFixture, HvfEngineSession, FBLayerView, URL) throws -> Void) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            let path = fixture.evidence.appendingPathComponent("display.fb")
            try writeFrame(path)
            let view = FBLayerView(session: session, imageFactory: factory)
            view.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
            defer { view.teardown() }
            view.refreshFrame(at: 0)
            XCTAssertTrue(view.guestInputAvailable)
            try body(fixture, session, view, path)
        }
    }

    static func writeFrame(_ path: URL, pixel: UInt32 = 0xff112233, sequence: UInt64 = 2) throws {
        var frame = Data(count: 68)
        for (offset, value): (Int, UInt32) in [(0, 0x42564642), (4, 1), (8, 1), (12, 1), (16, 4), (64, pixel)] {
            var little = value.littleEndian
            withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        var little = sequence.littleEndian
        withUnsafeBytes(of: &little) { frame.replaceSubrange(24..<32, with: $0) }
        try frame.write(to: path)
    }

    static func update<Value: FixedWidthInteger>(_ value: Value, offset: UInt64, path: URL) throws {
        let handle = try FileHandle(forWritingTo: path)
        defer { try? handle.close() }
        try handle.seek(toOffset: offset)
        var little = value.littleEndian
        try withUnsafeBytes(of: &little) { try handle.write(contentsOf: Data($0)) }
    }

    static func mouse(_ type: NSEvent.EventType) throws -> NSEvent {
        try XCTUnwrap(NSEvent.mouseEvent(with: type, location: CGPoint(x: 50, y: 50),
            modifierFlags: [], timestamp: 2, windowNumber: 0, context: nil,
            eventNumber: 0, clickCount: 1, pressure: 1))
    }

    static func mark(_ view: FBLayerView) {
        view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertTrue(view.hasMarkedText())
    }

    static func image(_ view: FBLayerView) throws -> CGImage {
        let contents = try XCTUnwrap(view.layer?.contents)
        XCTAssertEqual(CFGetTypeID(contents as CFTypeRef), CGImage.typeID)
        return contents as! CGImage
    }
}
