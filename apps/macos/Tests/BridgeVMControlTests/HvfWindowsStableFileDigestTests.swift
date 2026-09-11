import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsStableFileDigestTests: XCTestCase {
    func testStandardDigestVectorsRemainCompatible() throws {
        try fixture { url in
            try Data("abc".utf8).write(to: url)
            XCTAssertEqual(HvfWindowsInstallCacheIdentity.sha256File(url.path),
                           "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
            try Data().write(to: url)
            XCTAssertEqual(HvfWindowsInstallCacheIdentity.sha256File(url.path),
                           "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        }
    }

    func testRejectsSymlinkDirectoryAndFIFO() throws {
        try fixture { url in
            try Data("abc".utf8).write(to: url)
            let link = url.appendingPathExtension("link")
            let fifo = url.appendingPathExtension("fifo")
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: url)
            XCTAssertNil(HvfWindowsInstallCacheIdentity.sha256File(link.path))
            XCTAssertNil(HvfWindowsInstallCacheIdentity.sha256File(url.deletingLastPathComponent().path))
            XCTAssertEqual(mkfifo(fifo.path, S_IRUSR | S_IWUSR), 0)
            XCTAssertNil(HvfWindowsInstallCacheIdentity.sha256File(fifo.path))
        }
    }

    func testPathReplacementDuringReadIsRejected() throws {
        try changedDuringRead { url in
            try Data("xyz".utf8).write(to: url, options: .atomic)
        }
    }

    func testGrowthDuringReadIsRejected() throws {
        try changedDuringRead { url in
            let handle = try FileHandle(forWritingTo: url)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: Data("extra".utf8))
        }
    }

    func testSameSizeMutationDuringReadIsRejected() throws {
        try changedDuringRead { url in
            let handle = try FileHandle(forWritingTo: url)
            try handle.write(contentsOf: Data("xyz".utf8))
            try handle.close()
            try FileManager.default.setAttributes(
                [.modificationDate: Date(timeIntervalSince1970: 1)], ofItemAtPath: url.path)
        }
    }

    func testPrematureEOFIsRejected() throws {
        try fixture { url in
            try Data("abc".utf8).write(to: url)
            XCTAssertThrowsError(try HvfWindowsStableFileDigest.compute(url.path) { _ in Data() })
        }
    }

    func testReadFailureDoesNotProduceDigest() throws {
        try fixture { url in
            try Data("abc".utf8).write(to: url)
            XCTAssertThrowsError(try HvfWindowsStableFileDigest.compute(url.path) { _ in
                throw CocoaError(.fileReadUnknown)
            })
        }
    }

    private func changedDuringRead(_ change: (URL) throws -> Void) throws {
        try fixture { url in
            try Data("abc".utf8).write(to: url)
            var changed = false
            XCTAssertThrowsError(try HvfWindowsStableFileDigest.compute(url.path) { handle in
                let data = try handle.read(upToCount: 8) ?? Data()
                if !changed { changed = true; try change(url) }
                return data
            })
            XCTAssertTrue(changed)
        }
    }

    private func fixture(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root.appendingPathComponent("artifact"))
    }
}
