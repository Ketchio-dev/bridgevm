import XCTest
@testable import BridgeVMProductE2E

/// The product installs an imported key before it publishes vm.json, so an interrupted import leaves a key under a
/// directory without one, or under the requested slug with nothing published. Cleanup counts a key as gone only on
/// the product's exact proof or its exact missing-key refusal, and a forget that never ends must not hang the lane.
final class A9KeyCleanupProofTests: XCTestCase {
    func testInterruptedImportKeyIsForgotten() throws {
        let fixture = try proofFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        try FileManager.default.createDirectory(at: fixture.config.deletingLastPathComponent(), withIntermediateDirectories: true)
        try installKey(fixture.request.vmSlug, fixture)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertFalse(hasKey(fixture.request.vmSlug, fixture))
    }

    func testRequestedSlugKeyIsForgottenWithoutALibrary() throws {
        let fixture = try proofFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        try installKey(fixture.request.vmSlug, fixture)
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        XCTAssertFalse(hasKey(fixture.request.vmSlug, fixture))
    }

    func testOnlyTheProductMissingKeyRefusalCountsAsAbsent() throws {
        let fixture = try proofFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        XCTAssertTrue(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
        try stub("echo 'ERROR: keychain read failed' >&2; exit 1", fixture)
        XCTAssertFalse(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
    }

    func testExitZeroWithoutTheExactProofIsNotVerified() throws {
        let fixture = try proofFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        try stub("echo forgot_imported_key=windows-11; exit 0", fixture)
        XCTAssertFalse(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default))
    }

    func testHungForgetFailsWithinItsBound() throws {
        let fixture = try proofFixture()
        defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        try stub("exec sleep 30", fixture)
        let started = Date()
        XCTAssertFalse(A9ImportedKeyCleanup.run(request: fixture.request, fileManager: .default, timeout: 1))
        XCTAssertLessThan(Date().timeIntervalSince(started), 10)
    }

    func testMissingKeyLineIsTheProductRefusal() throws {
        let sources = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().appendingPathComponent("Sources/BridgeVMControl/HvfEngine")
        let message = String(A9KeyForgetCall.missingKeyLine.dropFirst("ERROR: ".count))
        let security = try String(contentsOf: sources.appendingPathComponent("VTPMStateSecurity.swift"), encoding: .utf8)
        XCTAssertTrue(security.contains("case .missingKeyForExistingState:\n            return \"\(message)\""))
        let command = try String(contentsOf: sources.appendingPathComponent("VTPMImportCleanupCommand.swift"), encoding: .utf8)
        XCTAssertTrue(command.contains("Data(\"ERROR: \\(error.localizedDescription)\\n\".utf8)"))
    }

    private func proofFixture() throws -> A9CleanupFixture {
        let fixture = try A9CleanupFixture()
        try Data(A9CleanupStubApp.script(capture: fixture.capture, keychain: fixture.keychain).utf8)
            .write(to: URL(fileURLWithPath: fixture.request.appExecutablePath))
        return fixture
    }

    private func stub(_ body: String, _ fixture: A9CleanupFixture) throws {
        try Data("#!/bin/sh\n\(body)\n".utf8).write(to: URL(fileURLWithPath: fixture.request.appExecutablePath))
    }

    private func installKey(_ slug: String, _ fixture: A9CleanupFixture) throws {
        try Data().write(to: fixture.keychain.appendingPathComponent(slug))
    }

    private func hasKey(_ slug: String, _ fixture: A9CleanupFixture) -> Bool {
        FileManager.default.fileExists(atPath: fixture.keychain.appendingPathComponent(slug).path)
    }
}
