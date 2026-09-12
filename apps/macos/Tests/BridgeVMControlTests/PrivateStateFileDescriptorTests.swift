import Foundation
import Darwin
import XCTest
@testable import BridgeVMControl

final class PrivateStateFileDescriptorTests: XCTestCase {
    func testCreatedDescriptorClosesOnExecAndDoesNotReplaceExistingFile() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("private-state")
        let descriptor = try PrivateStateFileDescriptor.create(at: file)
        defer { Darwin.close(descriptor) }
        let flags = fcntl(descriptor, F_GETFD)
        XCTAssertGreaterThanOrEqual(flags, 0)
        XCTAssertNotEqual(flags & FD_CLOEXEC, 0)
        var metadata = stat()
        XCTAssertEqual(fstat(descriptor, &metadata), 0)
        XCTAssertEqual(metadata.st_mode & 0o777, 0o600)
        XCTAssertThrowsError(try PrivateStateFileDescriptor.create(at: file))
        let alias = root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: file)
        XCTAssertThrowsError(try PrivateStateFileDescriptor.create(at: alias))
    }
}
