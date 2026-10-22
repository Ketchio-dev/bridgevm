import XCTest
@testable import BridgeVMProductE2E

final class A9ImportedKeyCleanupTests: XCTestCase {
    func testCleanupAccountsForTheRequestedSlugBeforeAndAfterPublication() throws {
        let fixture = try A9CleanupFixture(), key = fixture.keychain.appendingPathComponent(fixture.request.vmSlug)
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertTrue(try String(contentsOf: fixture.capture, encoding: .utf8).contains("--stable-vm-id \(fixture.request.vmSlug) "))
        try FileManager.default.createDirectory(at: fixture.config.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: fixture.config); try Data().write(to: key)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertTrue(try String(contentsOf: fixture.capture, encoding: .utf8).contains("--vtpm-lifecycle forget-import"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: key.path))
    }
}
