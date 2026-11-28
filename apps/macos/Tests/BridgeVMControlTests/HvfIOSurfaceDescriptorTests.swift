import IOSurface
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
        let dir = LibraryDeletionTestDirectory.temporary
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

final class HvfIOSurfacePresenterTests: XCTestCase {
    /// The 2D display (the default) has no sidecar: the display tick looks for it about once a second, not ten
    /// times. A shown sidecar that reads empty while the VMM rewrites it keeps the 10 Hz pace.
    func testAMissingSidecarIsLookedForAboutOnceASecond() throws {
        let dir = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let sidecar = dir.appendingPathComponent("display.fb.iosurface")
        let surface = try XCTUnwrap(HvfFramebufferSurfaces.surface(width: 8, height: 8))
        let descriptor = "\(IOSurfaceGetID(surface)) 8 8\n"
        var presenter = HvfIOSurfacePresenter()
        try withExtendedLifetime(surface) {
            XCTAssertNil(presenter.presentation(from: sidecar, at: 10))
            try descriptor.write(to: sidecar, atomically: true, encoding: .utf8)
            XCTAssertNil(presenter.presentation(from: sidecar, at: 10.5), "a missing sidecar is not read on every tick")
            XCTAssertEqual(presenter.presentation(from: sidecar, at: 11)?.width, 8, "3D shows within about a second")
            try "".write(to: sidecar, atomically: false, encoding: .utf8)
            XCTAssertNil(presenter.presentation(from: sidecar, at: 11.125))
            try descriptor.write(to: sidecar, atomically: false, encoding: .utf8)
            XCTAssertEqual(presenter.presentation(from: sidecar, at: 11.25)?.width, 8)
        }
    }
}
