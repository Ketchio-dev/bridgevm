import Foundation
import XCTest
@testable import BridgeVMControl

/// Duplicate and Move to another drive copy the whole bundle. With too little room there, the copy ran
/// until it filled that drive (pausing any VM running from it), then failed with a sentence that read as
/// if the VM had broken.
@MainActor
final class LibraryCopySpaceTests: XCTestCase {
    func testMoveAndDuplicateAcrossDrivesSayTheDriveIsFullBeforeCopying() async throws {
        let drive = try LibraryCopySpaceDrive()
        defer { drive.detach() }
        let f = try LibraryMoveSelectionFixture()
        let none: @Sendable (URL) -> Int64? = { _ in 0 }
        let plenty: @Sendable (URL) -> Int64? = { _ in 1 << 40 }
        let onDrive = drive.mount.appendingPathComponent("bundle.vmbridge")

        f.library.moveWindowsHVFBundle(f.source, to: drive.mount, available: none)
        try await f.finish()
        XCTAssertEqual(f.library.moveError, "“\(LibraryCopySpaceDrive.name)” doesn’t have enough free space for this VM. Free up space, or choose another drive.")
        XCTAssertFalse(FileManager.default.fileExists(atPath: onDrive.path), "nothing is copied")
        try f.assets.assertReadable(f.source)
        XCTAssertFalse(VMRelocationJournal.isPending(f.source, rootURL: f.assets.libraryRoot))

        f.library.moveError = nil
        f.library.moveWindowsHVFBundle(f.source, to: drive.mount, available: plenty)
        try await f.finish()
        XCTAssertNil(f.library.moveError)
        let moved = try XCTUnwrap(f.library.vms.first { $0.slug == f.source.slug })
        XCTAssertEqual(moved.bundlePath, onDrive.path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.source.bundlePath))

        // Duplicate copies it back to the library's drive. Never copy media or touch keys here.
        let calls = LibraryCopySpaceCalls()
        let operation: LibraryCloneOperation = { _, _, _ in calls.count += 1; return nil }
        f.library.cloneWindowsHVF(moved, name: "copy", operation: operation, available: none)
        try await f.finish()
        let library = try XCTUnwrap(f.assets.libraryRoot.resourceValues(forKeys: [.volumeNameKey]).volumeName)
        XCTAssertEqual(f.library.cloneError, "“\(library)” doesn’t have enough free space for a copy of this VM. Free up space, then try again.")
        XCTAssertEqual(calls.count, 0, "nothing is copied")

        f.library.cloneError = nil
        f.library.cloneWindowsHVF(moved, name: "copy", operation: operation, available: plenty)
        try await f.finish()
        XCTAssertEqual(calls.count, 1)
        XCTAssertFalse(f.library.cloneError?.contains("free space for a copy of this VM") ?? false)
    }
}

private final class LibraryCopySpaceCalls: @unchecked Sendable { var count = 0 }

/// A small HFS+ disk image: a real second drive, and one that can't hold sparse files.
private final class LibraryCopySpaceDrive {
    static let name = "BridgeVM Copy Space Test"
    let root: URL
    let mount: URL

    init() throws {
        root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bv-copy-space-\(UUID().uuidString)", isDirectory: true)
        mount = root.appendingPathComponent("drive", isDirectory: true)
        try FileManager.default.createDirectory(at: mount, withIntermediateDirectories: true)
        let image = root.appendingPathComponent("drive.dmg")
        try Self.hdiutil(["create", "-quiet", "-size", "16m", "-fs", "HFS+", "-volname", Self.name, "-o", image.path])
        try Self.hdiutil(["attach", "-quiet", "-nobrowse", "-noverify", "-noautofsck", "-mountpoint", mount.path, image.path])
    }

    func detach() {
        if (try? Self.hdiutil(["detach", "-quiet", mount.path])) == nil {
            try? Self.hdiutil(["detach", "-quiet", "-force", mount.path])
        }
        try? FileManager.default.removeItem(at: root)
    }

    private static func hdiutil(_ arguments: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/hdiutil")
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw NSError(domain: "hdiutil", code: Int(process.terminationStatus),
                          userInfo: [NSLocalizedDescriptionKey: arguments.joined(separator: " ")])
        }
    }
}
