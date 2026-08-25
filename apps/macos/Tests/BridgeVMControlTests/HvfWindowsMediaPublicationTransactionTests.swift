import XCTest
@testable import BridgeVMControl

final class HvfWindowsMediaPublicationTransactionTests: XCTestCase {
    func testSecondSwapFailureRollsBackFirstSwap() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = (root.appendingPathComponent("disk"), root.appendingPathComponent("new-disk"))
        let vars = (root.appendingPathComponent("vars"), root.appendingPathComponent("new-vars"))
        try Data("old-disk".utf8).write(to: disk.0); try Data("new-disk".utf8).write(to: disk.1)
        try Data("old-vars".utf8).write(to: vars.0); try Data("new-vars".utf8).write(to: vars.1)
        var swaps = 0
        XCTAssertThrowsError(try HvfWindowsMediaPublicationTransaction.publish(
            [(disk.0.path, disk.1.path), (vars.0.path, vars.1.path)], swapping: { item in
                if swaps == 1 { throw HvfWindowsAtomicMediaPublisher.PublicationError.stagingFailed }
                swaps += 1; try HvfWindowsAtomicMediaPublisher.swap(item)
            }))
        XCTAssertEqual(try Data(contentsOf: disk.0), Data("old-disk".utf8))
        XCTAssertEqual(try Data(contentsOf: vars.0), Data("old-vars".utf8))
        XCTAssertTrue(FileManager.default.fileExists(atPath: disk.1.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: vars.1.path))
        XCTAssertFalse(try FileManager.default.contentsOfDirectory(atPath: root.path)
            .contains(where: { $0.contains(".staging-") }))
    }
}
