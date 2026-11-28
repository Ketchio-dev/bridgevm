@testable import BridgeVMControl
import Foundation
import XCTest

final class VMLibraryCopyTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("bv-copy-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    /// A copy that can't clone (another volume) keeps a VM disk's holes: a Duplicate of a VM on
    /// another drive used to write the disk's whole logical size to the library volume.
    func testCopyKeepsADiskSparseWithTheSameBytesAndModes() throws {
        let source = root.appendingPathComponent("bundle.vmbridge")
        let disks = source.appendingPathComponent("disks")
        try FileManager.default.createDirectory(at: disks, withIntermediateDirectories: true)
        let disk = disks.appendingPathComponent("hvf-target.raw")
        let mib = 1 << 20
        XCTAssertTrue(FileManager.default.createFile(atPath: disk.path, contents: nil))
        let handle = try FileHandle(forWritingTo: disk)
        try handle.write(contentsOf: Data(repeating: 0xa5, count: mib))
        try handle.seek(toOffset: UInt64(64 * mib))
        try handle.write(contentsOf: Data(repeating: 0x5a, count: mib))
        try handle.truncate(atOffset: UInt64(128 * mib))
        try handle.close()
        let vars = source.appendingPathComponent("vars.fd")
        XCTAssertTrue(FileManager.default.createFile(atPath: vars.path, contents: Data([1, 2, 3]),
                                                    attributes: [.posixPermissions: 0o600]))

        let copy = root.appendingPathComponent("copy.vmbridge")
        XCTAssertTrue(VMLibrary.cloneOrCopyFile(from: source.path, to: copy.path, clone: false))

        let copiedDisk = copy.appendingPathComponent("disks/hvf-target.raw")
        let values = try copiedDisk.resourceValues(forKeys: [.fileSizeKey, .totalFileAllocatedSizeKey])
        XCTAssertEqual(values.fileSize, 128 * mib)
        XCTAssertLessThanOrEqual(try XCTUnwrap(values.totalFileAllocatedSize), 16 * mib)
        XCTAssertEqual(try Data(contentsOf: copiedDisk), try Data(contentsOf: disk))
        let copiedVars = copy.appendingPathComponent("vars.fd")
        XCTAssertEqual(try Data(contentsOf: copiedVars), Data([1, 2, 3]))
        let mode = try FileManager.default.attributesOfItem(atPath: copiedVars.path)[.posixPermissions] as? Int
        XCTAssertEqual(mode, 0o600)
    }

    /// Moving a VM to another drive keeps its disk's holes and removes the source afterwards.
    /// BRIDGEVM_TEST_SECOND_VOLUME names a folder on another volume than the temporary folder, to
    /// make it a real move across volumes; otherwise the across-volume path runs on one volume.
    func testMoveAcrossVolumesKeepsTheDiskSparseAndRemovesTheSource() throws {
        let source = try sparseBundle(named: "source.vmbridge")
        let disk = source.appendingPathComponent("disks/hvf-target.raw")
        let expected = try Data(contentsOf: disk)
        let parent: URL
        if let other = ProcessInfo.processInfo.environment["BRIDGEVM_TEST_SECOND_VOLUME"] {
            parent = URL(fileURLWithPath: other).appendingPathComponent("bv-move-\(UUID().uuidString)")
            addTeardownBlock { try? FileManager.default.removeItem(at: parent) }
        } else {
            parent = root.appendingPathComponent("other")
        }
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        let moved = parent.appendingPathComponent("bundle.vmbridge")

        let forced: Bool? = ProcessInfo.processInfo.environment["BRIDGEVM_TEST_SECOND_VOLUME"] == nil ? true : nil
        try VMLibrary.relocateItem(at: source, to: moved, fileManager: .default, acrossVolumes: forced)

        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
        let movedDisk = moved.appendingPathComponent("disks/hvf-target.raw")
        let values = try movedDisk.resourceValues(forKeys: [.fileSizeKey, .totalFileAllocatedSizeKey])
        XCTAssertEqual(values.fileSize, 128 << 20)
        XCTAssertLessThanOrEqual(try XCTUnwrap(values.totalFileAllocatedSize), 16 << 20)
        XCTAssertEqual(try Data(contentsOf: movedDisk), expected)

        let again = try sparseBundle(named: "again.vmbridge")
        XCTAssertThrowsError(try VMLibrary.relocateItem(at: again, to: moved, fileManager: .default, acrossVolumes: true))
        XCTAssertTrue(FileManager.default.fileExists(atPath: again.appendingPathComponent("disks/hvf-target.raw").path),
                      "A refused move keeps the source")
    }

    private func sparseBundle(named name: String) throws -> URL {
        let bundle = root.appendingPathComponent(name)
        let disks = bundle.appendingPathComponent("disks")
        try FileManager.default.createDirectory(at: disks, withIntermediateDirectories: true)
        let disk = disks.appendingPathComponent("hvf-target.raw")
        XCTAssertTrue(FileManager.default.createFile(atPath: disk.path, contents: nil))
        let handle = try FileHandle(forWritingTo: disk)
        try handle.write(contentsOf: Data(repeating: 0xa5, count: 1 << 20))
        try handle.seek(toOffset: UInt64(64 << 20))
        try handle.write(contentsOf: Data(repeating: 0x5a, count: 1 << 20))
        try handle.truncate(atOffset: UInt64(128 << 20))
        try handle.close()
        return bundle
    }

    func testCopyNeverWritesIntoAnExistingDestination() throws {
        let source = root.appendingPathComponent("source")
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        try Data("new".utf8).write(to: source.appendingPathComponent("file"))
        let existing = root.appendingPathComponent("existing")
        try FileManager.default.createDirectory(at: existing, withIntermediateDirectories: true)
        try Data("kept".utf8).write(to: existing.appendingPathComponent("file"))

        XCTAssertFalse(VMLibrary.cloneOrCopyFile(from: source.path, to: existing.path))
        XCTAssertEqual(try String(contentsOf: existing.appendingPathComponent("file"), encoding: .utf8), "kept")
    }
}
