import XCTest; @testable import BridgeVMControl
extension HvfWindowsKernelPolicyVerifierTests {
    func assertUnsafeSnapshotPathsRefused() throws {
        let fixture = try makeFixture()
        let realParent = fixture.root.appendingPathComponent("real-parent", isDirectory: true)
        try FileManager.default.createDirectory(at: realParent, withIntermediateDirectories: false)
        let nested = realParent.appendingPathComponent("nested", isDirectory: true)
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: false)
        for path in [realParent, nested] {
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: path.path)
        }
        let linked = fixture.root.appendingPathComponent("linked-parent", isDirectory: true)
        try FileManager.default.createSymbolicLink(at: linked, withDestinationURL: realParent)
        for parent in [linked, linked.appendingPathComponent("nested")] {
            assertFailure(HvfWindowsKernelPolicyVerifier.stageVerifiedSnapshot(
                from: fixture.package, to: parent.appendingPathComponent("snapshot"),
                now: fixture.now, trustAnchors: [fixture.anchor]), .snapshotInvalid)
        }
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: nested.appendingPathComponent("snapshot").path))
        let broken = fixture.root.appendingPathComponent("broken-snapshot")
        try FileManager.default.createSymbolicLink(
            at: broken, withDestinationURL: fixture.root.appendingPathComponent("missing"))
        assertFailure(HvfWindowsKernelPolicyVerifier.stageVerifiedSnapshot(
            from: fixture.package, to: broken, now: fixture.now,
            trustAnchors: [fixture.anchor]), .snapshotInvalid)
    }
}
