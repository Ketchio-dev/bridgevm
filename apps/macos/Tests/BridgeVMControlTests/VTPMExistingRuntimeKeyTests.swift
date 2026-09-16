import Foundation
import Security
import XCTest
@testable import BridgeVMControl

final class VTPMExistingRuntimeKeyTests: XCTestCase {
    func testEmptyAndMissingStateStillRequireExistingKeyWithoutWrites() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("existing-key-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for state in [root, root.appendingPathComponent("absent")] {
            let fixture = VTPMKeychainFixture(); fixture.read = (nil, errSecItemNotFound)
            var consumed = false
            XCTAssertThrowsError(try VTPMExistingRuntimeKey.withExistingKey(for: config(state.path), provider: fixture.store) { _ in consumed = true }) {
                XCTAssertEqual($0 as? VTPMExistingKeyError, .missingKey(.legacySearchList))
            }
            XCTAssertFalse(consumed); XCTAssertTrue(fixture.writes.isEmpty)
        }
    }

    func testInvalidStatePathRefusesBeforeAnySecurityOperation() throws {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data([1]).write(to: path); defer { try? FileManager.default.removeItem(at: path) }
        let fixture = VTPMKeychainFixture()
        XCTAssertThrowsError(try VTPMExistingRuntimeKey.withExistingKey(for: config(path.path), provider: fixture.store) { _ in XCTFail("Invalid state consumed a key") })
        XCTAssertTrue(fixture.calls.isEmpty)
    }

    func testConsumingBodyRunsAfterPolicyRestorationAndGateRelease() throws {
        let fixture = VTPMKeychainFixture()
        let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let result = try VTPMExistingRuntimeKey.withExistingKey(for: config(path), provider: fixture.store) { key in
            XCTAssertTrue(fixture.allowed)
            XCTAssertEqual(try fixture.access.exclusively { 42 }, 42)
            return try XCTUnwrap(key).count
        }
        XCTAssertEqual(result, 32); XCTAssertTrue(fixture.writes.isEmpty)
    }

    func testRestoreFailureOrMalformedKeyNeverReachesConsumingBody() {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        for restoreFails in [false, true] {
            let fixture = VTPMKeychainFixture()
            if restoreFails { fixture.setStatuses = [errSecSuccess, errSecNotAvailable] }
            else { fixture.read.data = Data(count: 31) }
            XCTAssertThrowsError(try VTPMExistingRuntimeKey.withExistingKey(for: config(path), provider: fixture.store) { _ in XCTFail("Refused key consumed") })
            XCTAssertTrue(fixture.writes.isEmpty)
        }
    }

    private func config(_ state: String) -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: "unused", uefiVarsPath: "unused", evidenceDir: "unused",
            watchdogMs: nil, ramMiB: 1024, smpCpus: 1, clipboardSync: false,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: "unused", vtpmStateDir: state, swtpmBin: "unused", vtpmKeyID: "stable-vm")
    }
}
