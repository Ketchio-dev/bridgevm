import XCTest
@testable import BridgeVMProductE2E

final class A9ImportedKeyCleanupTests: XCTestCase {
    func testCleanupRunsOnlyForPublishedImport() throws {
        let fixture = try A9CleanupFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.capture.path))
        try FileManager.default.createDirectory(at: fixture.config.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: fixture.config)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        let arguments = try String(contentsOf: fixture.capture, encoding: .utf8)
        XCTAssertTrue(arguments.contains("--vtpm-lifecycle forget-import"))
        XCTAssertTrue(arguments.contains("--stable-vm-id \(fixture.request.vmSlug)"))
    }
}
