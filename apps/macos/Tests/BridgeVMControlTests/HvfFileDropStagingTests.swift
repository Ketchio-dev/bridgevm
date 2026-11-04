import Foundation
import XCTest
@testable import BridgeVMControl

/// Staging copies only regular files and folders, never through a symlink, under Windows names.
final class HvfFileDropStagingTests: XCTestCase {
    private func scratch() throws -> URL {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent("bvdrop-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return base
    }

    func testOnlyRegularFilesAndFoldersAreStagedAndSymlinksAreNeverFollowed() throws {
        let base = try scratch()
        defer { try? FileManager.default.removeItem(at: base) }
        let fm = FileManager.default
        let folder = base.appendingPathComponent("source/My Folder")
        try fm.createDirectory(at: folder.appendingPathComponent("inner"), withIntermediateDirectories: true)
        try Data("hi".utf8).write(to: folder.appendingPathComponent("inner/a.txt"))
        try Data("x".utf8).write(to: folder.appendingPathComponent("CON.txt"))
        try Data().write(to: folder.appendingPathComponent(".DS_Store"))
        let outside = base.appendingPathComponent("outside")
        try fm.createDirectory(at: outside, withIntermediateDirectories: true)
        try Data("secret".utf8).write(to: outside.appendingPathComponent("secret.txt"))
        try fm.createSymbolicLink(at: folder.appendingPathComponent("escape"), withDestinationURL: outside)
        try fm.createSymbolicLink(at: folder.appendingPathComponent("inner/up.txt"),
                                  withDestinationURL: folder.appendingPathComponent("CON.txt"))
        let droppedLink = base.appendingPathComponent("source/link-to-outside")
        try fm.createSymbolicLink(at: droppedLink, withDestinationURL: outside)
        let decomposed = "자모.txt".decomposedStringWithCanonicalMapping
        let nfdFile = base.appendingPathComponent("source").appendingPathComponent(decomposed)
        try Data("nfd".utf8).write(to: nfdFile)

        let staging = base.appendingPathComponent("staging")
        let summary = try HvfFileDropStaging.stage([folder, droppedLink, nfdFile], into: staging)

        XCTAssertEqual(summary.files, 3)
        XCTAssertEqual(summary.folders, 2)
        XCTAssertEqual(summary.skipped, 3) // escape, inner/up.txt and the dropped link itself
        XCTAssertEqual(summary.bytes, 6)
        XCTAssertEqual(summary.cloned, 3) // the temporary directory is one APFS volume
        let staged = try fm.subpathsOfDirectory(atPath: staging.path).sorted()
        XCTAssertEqual(staged, ["My Folder", "My Folder/_CON.txt", "My Folder/inner", "My Folder/inner/a.txt", "자모.txt"])
        let stagedNFD = try fm.contentsOfDirectory(atPath: staging.path).first { $0.hasSuffix(".txt") }
        XCTAssertEqual(stagedNFD.map { Array($0.unicodeScalars) },
                       Array("자모.txt".precomposedStringWithCanonicalMapping.unicodeScalars))
        XCTAssertEqual(try Data(contentsOf: staging.appendingPathComponent("My Folder/inner/a.txt")), Data("hi".utf8))
    }

    func testAPlainCopyStandsInWhenTheVolumeCannotClone() throws {
        let base = try scratch()
        defer { try? FileManager.default.removeItem(at: base) }
        let file = base.appendingPathComponent("video.mov")
        try Data("abc".utf8).write(to: file)
        let staging = base.appendingPathComponent("staging")
        let summary = try HvfFileDropStaging.stage([file], into: staging, clone: { _, _ in false })
        XCTAssertEqual(summary.cloned, 0)
        XCTAssertEqual(summary.copied, 1)
        XCTAssertEqual(try Data(contentsOf: staging.appendingPathComponent("video.mov")), Data("abc".utf8))
    }

    func testTwoDroppedItemsWithOneWindowsNameBothLand() throws {
        let base = try scratch()
        defer { try? FileManager.default.removeItem(at: base) }
        let first = base.appendingPathComponent("a/note:1.txt")
        let second = base.appendingPathComponent("b/note_1.txt")
        for url in [first, second] {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(url.path.utf8).write(to: url)
        }
        let staging = base.appendingPathComponent("staging")
        _ = try HvfFileDropStaging.stage([first, second], into: staging)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: staging.path).sorted(),
                       ["note_1 (2).txt", "note_1.txt"])
    }
}
