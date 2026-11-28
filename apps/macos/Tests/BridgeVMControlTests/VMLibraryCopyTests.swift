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
        XCTAssertGreaterThanOrEqual(16 * mib, try XCTUnwrap(values.totalFileAllocatedSize))
        XCTAssertEqual(try Data(contentsOf: copiedDisk), try Data(contentsOf: disk))
        let copiedVars = copy.appendingPathComponent("vars.fd")
        XCTAssertEqual(try Data(contentsOf: copiedVars), Data([1, 2, 3]))
        let mode = try FileManager.default.attributesOfItem(atPath: copiedVars.path)[.posixPermissions] as? Int
        XCTAssertEqual(mode, 0o600)
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
