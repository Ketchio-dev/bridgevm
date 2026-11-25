import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallProcessTests: XCTestCase {
    func testFailingInstallerDeliversItsCompleteOutputBeforeReturning() async throws {
        let fixture = try HvfWindowsInstallStagingFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let command = #"exec /usr/bin/awk 'BEGIN { for (i = 0; i < 3000; i++) print "line-" i; printf "마지막 오류"; exit 23 }' >&2"#
        var lines: [String] = []
        let succeeded = await HvfWindowsInstallProcess().run(plan: fixture.plan,
            arguments: ["/bin/sh", "-c", command], environment: [:], progressLog: nil) { lines.append($0) }
        XCTAssertFalse(succeeded)
        // The launcher reports its command first. The child omits the final newline.
        let output = lines.dropFirst()
        XCTAssertEqual(output.count, 3001)
        XCTAssertTrue(output.dropLast().elementsEqual((0..<3000).map { "line-" + String($0) }))
        XCTAssertEqual(output.last, "마지막 오류")
    }
}
