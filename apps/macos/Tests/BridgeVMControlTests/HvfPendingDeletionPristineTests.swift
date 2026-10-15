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

    func testGroupOrWorldWritableDirectoriesRefuseBeforeDeletion() throws {
        for name in ["library", "entry", "bundle", "disks", "metadata"] {
            for mode in [0o770, 0o707] {
                let f = try HvfPendingDeletionFixture()
                let directory: URL
                switch name {
                case "library": directory = f.library
                case "entry": directory = f.entry
                case "bundle": directory = f.bundle
                default: directory = f.bundle.appendingPathComponent(name)
                }
                try f.fm.setAttributes([.posixPermissions: mode], ofItemAtPath: directory.path)
                defer { try? f.fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path) }
                try f.assertRefused()
            }
        }
    }
}
