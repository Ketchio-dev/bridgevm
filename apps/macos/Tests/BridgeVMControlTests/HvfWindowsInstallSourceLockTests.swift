import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallSourceLockTests: XCTestCase {
    func testLockIsExclusiveAndAutomaticallyRecoverable() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-source-lock-\(UUID())", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let image = root.appendingPathComponent("cache/source.raw").path
        var first: HvfWindowsInstallSourceLock? = try HvfWindowsInstallSourceLock(
            sourceImagePath: image)
        XCTAssertThrowsError(try HvfWindowsInstallSourceLock(sourceImagePath: image)) { error in
            XCTAssertEqual(error as? HvfWindowsInstallSourceLock.LockError, .busy)
        }
        first = nil
        XCTAssertNoThrow(try HvfWindowsInstallSourceLock(sourceImagePath: image))
        XCTAssertNil(first)
    }

    func testLockRejectsSymlinkedCacheDirectory() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-source-lock-link-\(UUID())", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(
            at: root.appendingPathComponent("cache"), withDestinationURL: URL(fileURLWithPath: "/tmp"))
        XCTAssertThrowsError(try HvfWindowsInstallSourceLock(
            sourceImagePath: root.appendingPathComponent("cache/source.raw").path))
    }
}
