import XCTest
@testable import BridgeVMControl

final class HvfIOSurfaceDescriptorTests: XCTestCase {
    func testParsesValidSidecar() throws {
        let descriptor = try XCTUnwrap(HvfIOSurfaceDescriptor.parse("42 1024 768\n"))
        XCTAssertEqual(descriptor.id, 42)
        XCTAssertEqual(descriptor.width, 1024)
        XCTAssertEqual(descriptor.height, 768)
    }

    func testRejectsMalformedOrUnsafeSidecars() {
        XCTAssertNil(HvfIOSurfaceDescriptor.parse(""))
        XCTAssertNil(HvfIOSurfaceDescriptor.parse("42 1024"))
        XCTAssertNil(HvfIOSurfaceDescriptor.parse("0 1024 768"))
        XCTAssertNil(HvfIOSurfaceDescriptor.parse("42 0 768"))
        XCTAssertNil(HvfIOSurfaceDescriptor.parse("42 1024 -1"))
        XCTAssertNil(HvfIOSurfaceDescriptor.parse("42 1024 768 trailing"))
    }

    func testLoadsBoundedSidecarFile() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let sidecar = dir.appendingPathComponent("display.fb.iosurface")
        try "7 800 600\n".write(to: sidecar, atomically: true, encoding: .utf8)
        XCTAssertEqual(HvfIOSurfaceDescriptor.load(from: sidecar)?.id, 7)

        try String(repeating: "1", count: 129).write(to: sidecar, atomically: true, encoding: .utf8)
        XCTAssertNil(HvfIOSurfaceDescriptor.load(from: sidecar))
    }
}

final class HvfIOSurfaceDescriptorPollerTests: XCTestCase {
    func testReloadsImmediatelyThenAtTenHertz() {
        var poller = HvfIOSurfaceDescriptorPoller()
        XCTAssertTrue(poller.reloadDue(at: 10))
        XCTAssertFalse(poller.reloadDue(at: 10.05))
        XCTAssertTrue(poller.reloadDue(at: 10.1))
    }

    func testResetMakesTheNextCheckImmediate() {
        var poller = HvfIOSurfaceDescriptorPoller()
        XCTAssertTrue(poller.reloadDue(at: 10))
        poller.reset()
        XCTAssertTrue(poller.reloadDue(at: 0))
    }
}
