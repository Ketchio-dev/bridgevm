import XCTest
@testable import BridgeVMProductE2E

/// T19 r1 registered `windows-11`; a cleanup that looked only under the requested slug leaked its key.
final class A9LaneRegistrationCleanupTests: XCTestCase {
    func testCleanupForgetsARegistrationUnderAnotherSlug() throws {
        let fixture = try A9CleanupFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        let other = try register("windows-11", fixture)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        let lines = try invocations(fixture)
        XCTAssertEqual(lines.count, 1)
        XCTAssertTrue(lines.first?.contains("--stable-vm-id windows-11 --state-dir \(other.path) ") == true)
        XCTAssertTrue(lines.first?.contains("--package package --recovery-code-file code") == true)
    }

    func testCleanupForgetsEveryRegistration() throws {
        let fixture = try A9CleanupFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        _ = try register(fixture.request.vmSlug, fixture); _ = try register("windows-11", fixture)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        let lines = try invocations(fixture)
        XCTAssertEqual(lines.count, 2)
        XCTAssertTrue(lines.contains { $0.contains("--stable-vm-id \(fixture.request.vmSlug) --state-dir \(fixture.request.vtpmStatePath) ") })
        XCTAssertTrue(lines.contains { $0.contains("--stable-vm-id windows-11 ") })
    }

    func testFailedForgetIsNotVerified() throws {
        let fixture = try A9CleanupFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        _ = try register("windows-11", fixture)
        try Data("#!/bin/sh\nexit 1\n".utf8).write(to: URL(fileURLWithPath: fixture.request.appExecutablePath))
        XCTAssertFalse(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
    }

    func testUnreadableLibraryIsNotVerified() throws {
        let fixture = try A9CleanupFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        for index in 0...A9LaneRegistrations.limit { _ = try register("vm-\(index)", fixture) }
        XCTAssertFalse(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.capture.path))
    }

    private func register(_ slug: String, _ fixture: A9CleanupFixture) throws -> URL {
        let directory = URL(fileURLWithPath: fixture.request.libraryRootPath).appendingPathComponent(slug)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: directory.appendingPathComponent("vm.json"))
        return directory.appendingPathComponent("bundle/metadata/vtpm")
    }

    private func invocations(_ fixture: A9CleanupFixture) throws -> [String] {
        try String(contentsOf: fixture.capture, encoding: .utf8).split(separator: "\n").map(String.init)
    }
}
