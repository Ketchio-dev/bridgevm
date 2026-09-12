#if canImport(AppKit)
import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfFramebufferFileIdentityTests: XCTestCase {
    func testSameSizeReplacementInvalidatesOldDescriptor() throws {
        try withDirectory { root in
            let path = root.appendingPathComponent("display.fb")
            try Data([1, 2, 3, 4]).write(to: path)
            let old = HvfFramebufferFileIdentity.open(path.path)
            defer { Darwin.close(old) }
            XCTAssertGreaterThanOrEqual(old, 0)
            XCTAssertEqual(HvfFramebufferFileIdentity.length(old, matching: path.path), 4)
            try Data([5, 6, 7, 8]).write(to: path, options: .atomic)
            XCTAssertNil(HvfFramebufferFileIdentity.length(old, matching: path.path))
            let fresh = HvfFramebufferFileIdentity.open(path.path)
            defer { Darwin.close(fresh) }
            XCTAssertEqual(HvfFramebufferFileIdentity.length(fresh, matching: path.path), 4)
            XCTAssertNotEqual(fcntl(fresh, F_GETFD) & FD_CLOEXEC, 0)
        }
    }

    func testLengthChangesAndRemovalAreObserved() throws {
        try withDirectory { root in
            let path = root.appendingPathComponent("display.fb")
            let descriptor = Darwin.open(path.path, O_RDWR | O_CREAT | O_EXCL, 0o600)
            defer { Darwin.close(descriptor) }
            XCTAssertGreaterThanOrEqual(descriptor, 0)
            XCTAssertEqual(ftruncate(descriptor, 64), 0)
            XCTAssertEqual(HvfFramebufferFileIdentity.length(descriptor, matching: path.path), 64)
            XCTAssertEqual(ftruncate(descriptor, 128), 0)
            XCTAssertEqual(HvfFramebufferFileIdentity.length(descriptor, matching: path.path), 128)
            try FileManager.default.removeItem(at: path)
            XCTAssertNil(HvfFramebufferFileIdentity.length(descriptor, matching: path.path))
        }
    }

    func testSymlinksDirectoriesAndFIFOsAreNotFramebuffers() throws {
        try withDirectory { root in
            let file = root.appendingPathComponent("real"), link = root.appendingPathComponent("link")
            try Data([1]).write(to: file)
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: file)
            let linked = HvfFramebufferFileIdentity.open(link.path)
            if linked >= 0 { Darwin.close(linked) }
            XCTAssertEqual(linked, -1)
            let fifo = root.appendingPathComponent("fifo")
            XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
            for path in [root, fifo] {
                let descriptor = HvfFramebufferFileIdentity.open(path.path)
                defer { if descriptor >= 0 { Darwin.close(descriptor) } }
                XCTAssertNil(HvfFramebufferFileIdentity.length(descriptor, matching: path.path))
            }
            XCTAssertNil(HvfFramebufferFileIdentity.length(-1, matching: file.path))
        }
    }

    private func withDirectory(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root)
    }
}
#endif
