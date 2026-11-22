import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionPristineTests: XCTestCase {
    func testRetainedVTPMStateIsNotAnUnstartedBundle() throws {
        for kind in ["directory", "file", "dangling"] {
            let f = try HvfPendingDeletionFixture()
            let path = "metadata/vtpm"
            if kind == "directory" { try f.directory(path) }
            else if kind == "file" { try f.file(path) }
            else { try f.danglingLink(path) }
            try f.assertRefused()
        }
    }

    /// exFAT and shared folders report group- and world-writable modes; that is no reason to keep the VM.
    func testGroupOrWorldWritableLibraryStillDeletesANeverStartedVM() throws {
        let f = try HvfPendingDeletionFixture()
        for directory in [f.library, f.entry, f.bundle] {
            try f.fm.setAttributes([.posixPermissions: 0o777], ofItemAtPath: directory.path)
        }
        defer { try? f.fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: f.library.path) }
        XCTAssertTrue(try f.delete())
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
    }
}
