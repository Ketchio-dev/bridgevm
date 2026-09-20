import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallSourceLockTemporaryAliasTests: XCTestCase {
    func testLockAcceptsExistingMacOSTemporaryAlias() throws {
        let root = URL(fileURLWithPath: "/private/tmp/bridgevm-source-lock-alias-\(UUID())", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let physical = root.appendingPathComponent("cache/source.raw").path
        var first: HvfWindowsInstallSourceLock? = try HvfWindowsInstallSourceLock(sourceImagePath: physical)
        XCTAssertNotNil(first)
        first = nil
        let lexical = physical.replacingOccurrences(of: "/private/tmp/", with: "/tmp/", options: .anchored)
        XCTAssertNoThrow(try HvfWindowsInstallSourceLock(sourceImagePath: lexical))
    }
}
