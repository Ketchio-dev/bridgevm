import XCTest
@testable import BridgeVMControl

final class HvfWindowsSnapshotQuotaTests: XCTestCase {
    func testExactSizesAndZeroMemberAreAcceptedWithoutOverflow() throws {
        for (output, expected) in [
            ("disk_bytes 128\nvars_bytes 16\n", UInt64(144)),
            ("disk_bytes 0\nvars_bytes 4\n", UInt64(4)),
            ("disk_bytes 4\nvars_bytes 0\n", UInt64(4)),
            ("disk_bytes 18446744073709551615\nvars_bytes 0\n", UInt64.max),
        ] {
            XCTAssertEqual(try HvfWindowsSnapshotQuota.parse(output), expected)
        }
    }

    func testMalformedIncompleteOrOverflowingSizesNeverBecomeAQuota() {
        for output in [
            "", "disk_bytes 1\n", "disk_bytes 1\nvars_bytes 2",
            "disk_bytes 1\nvars_bytes 2\n\n", "disk_bytes 1\nvars_bytes 2\nextra 3\n",
            "vars_bytes 2\ndisk_bytes 1\n", "disk_bytes 1\ndisk_bytes 2\n",
            "disk_bytes +1\nvars_bytes 2\n", "disk_bytes -1\nvars_bytes 2\n",
            "disk_bytes 01\nvars_bytes 2\n", "disk_bytes  1\nvars_bytes 2\n",
            "disk_bytes 1 \nvars_bytes 2\n", "disk_bytes １\nvars_bytes 2\n",
            "disk_bytes 1\r\nvars_bytes 2\r\n", "disk_bytes 0\nvars_bytes 0\n",
            "disk_bytes 18446744073709551616\nvars_bytes 0\n",
            "disk_bytes 18446744073709551615\nvars_bytes 1\n",
        ] {
            XCTAssertThrowsError(try HvfWindowsSnapshotQuota.parse(output), output)
        }
    }

    func testSizingFailureHasNoFallbackAndRestoreDoesNotQuerySize() throws {
        let plan = HvfWindowsSnapshotCommand.Plan(
            executable: URL(fileURLWithPath: "/usr/bin/false"), disk: URL(fileURLWithPath: "/disk"),
            vars: URL(fileURLWithPath: "/vars"), snapshot: URL(fileURLWithPath: "/snapshot"), vmID: "vm")
        XCTAssertThrowsError(try plan.arguments(for: .create))
        XCTAssertEqual(try plan.arguments(for: .restore), ["restore", "/snapshot", "/disk", "/vars"])
    }

    func testAdmissionRevokedDuringSizeQueryPreventsCreate() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = root.appendingPathComponent("helper")
        let marker = root.appendingPathComponent("sized")
        let script = """
        #!/bin/sh
        cd -- "$(dirname -- "$0")" || exit 1
        if [ "$1" = size ]; then
          touch sized
          printf 'disk_bytes 128\\nvars_bytes 16\\n'
        else
          touch unexpected-create
          exit 1
        fi
        """
        try Data(script.utf8).write(to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        let plan = HvfWindowsSnapshotCommand.Plan(
            executable: helper, disk: root.appendingPathComponent("disk"),
            vars: root.appendingPathComponent("vars"), snapshot: root.appendingPathComponent("snapshot"),
            vmID: "synthetic")
        do {
            _ = try await HvfWindowsSnapshotCommand.run(.create, plan: plan) {
                if FileManager.default.fileExists(atPath: marker.path) { throw CocoaError(.userCancelled) }
            }
            XCTFail("revoked admission must not dispatch create")
        } catch { XCTAssertEqual((error as NSError).code, CocoaError.userCancelled.rawValue) }
        XCTAssertTrue(FileManager.default.fileExists(atPath: marker.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("unexpected-create").path))
    }
}
