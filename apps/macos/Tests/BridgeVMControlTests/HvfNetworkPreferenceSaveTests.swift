import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfNetworkPreferenceSaveTests: XCTestCase {
    private func lines(_ data: Data, excluding key: String) -> [Substring] {
        String(decoding: data, as: UTF8.self).split(separator: "\n").filter { !$0.contains("\"\(key)\" :") }
    }

    /// Creation writes an explicit `networkEnabled: true`; off then on restores those exact bytes.
    func testOffChangesOnlyTheNetworkKeyAndOnRestoresCreatedBytesAndDigest() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.networkEnabled = true; fixture.save(saved)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let canonical = library.latestConfiguration(for: saved)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: canonical)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))

        XCTAssertTrue(session.applyNetworkToggle(false) == (false, nil))
        let off = try Data(contentsOf: url)
        XCTAssertTrue(String(decoding: off, as: UTF8.self).contains("\"networkEnabled\" : false"))
        XCTAssertEqual(lines(off, excluding: "networkEnabled"), lines(original, excluding: "networkEnabled"),
                       "Every other vm.json line is byte-for-byte unchanged")
        XCTAssertNotEqual(try NativeRuntimeConfigurationIdentity.digest(config: library.latestConfiguration(for: saved)), digest)

        XCTAssertTrue(session.applyNetworkToggle(true) == (true, nil))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertEqual(library.latestConfiguration(for: saved), canonical)
        XCTAssertEqual(try NativeRuntimeConfigurationIdentity.digest(config: library.latestConfiguration(for: saved)), digest)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertTrue(session.config.virtioNet)
        XCTAssertEqual(probe.sessions, 1); XCTAssertEqual(probe.lookups.count, 2)
    }

    /// A legacy registration without the key is already on: turning it on writes nothing.
    func testEnablingAnAbsentKeyWritesNothing() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        XCTAssertFalse(String(decoding: original, as: UTF8.self).contains("networkEnabled"))
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertTrue(session.applyNetworkToggle(true) == (true, nil))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertTrue(probe.lookups.isEmpty, "An unchanged value never reaches the save checks")
    }

    func testToggleIsDisabledAndSaveRefusedWhileRunningOrBusy() throws {
        for state in [HvfConnectionState.booting, .connected(host: "fixture"), .stopping, .timedOut] {
            let fixture = try HvfRuntimeSessionStoreFixture()
            defer { fixture.clean() }
            let saved = fixture.config(); fixture.save(saved)
            let url = fixture.registration(saved), original = try Data(contentsOf: url)
            let library = fixture.library()
            let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
            let toggle = HvfNetworkToggle(session: session, isOn: .constant(true))
            XCTAssertFalse(toggle.isDisabled)
            session.connectionState = state
            XCTAssertTrue(toggle.isDisabled, "\(state)")
            XCTAssertNotNil(session.saveNetworkPreference(false), "\(state)")
            XCTAssertTrue(session.applyNetworkToggle(false).isOn, "\(state)")
            XCTAssertEqual(try Data(contentsOf: url), original)
            XCTAssertNil(library.latestConfiguration(for: saved).networkEnabled)
            XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
            XCTAssertTrue(session.config.virtioNet)
        }
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .snapshot))
        XCTAssertTrue(HvfNetworkToggle(session: session, isOn: .constant(true)).isDisabled)
        XCTAssertNotNil(session.saveNetworkPreference(false))
        session.finishRuntimeMutation(reservation)
        session.attachedToExistingProcess = true
        XCTAssertTrue(HvfNetworkToggle(session: session, isOn: .constant(true)).isDisabled)
        XCTAssertNotNil(session.saveNetworkPreference(false), "An attached VM is not stopped")
        session.attachedToExistingProcess = false
        XCTAssertNil(library.latestConfiguration(for: saved).networkEnabled)
    }

    func testUnattachedRunningProcessRefusesSave() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let original = try Data(contentsOf: fixture.registration(saved))
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        probe.running = true
        XCTAssertTrue(session.applyNetworkToggle(false).isOn)
        XCTAssertEqual(probe.lookups, [session.config.targetDiskPath])
        XCTAssertEqual(try Data(contentsOf: fixture.registration(saved)), original)
        XCTAssertTrue(session.config.virtioNet)
    }

    func testExperimentalEntryKeepsViewLocalToggle() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let library = fixture.library()
        let session = library.experimentalHvfRuntimeSession()
        session.connectionState = .booting
        XCTAssertFalse(HvfNetworkToggle(session: session, isOn: .constant(true)).isDisabled)
        XCTAssertNil(session.saveNetworkPreference(true))
        XCTAssertTrue(session.applyNetworkToggle(true) == (true, nil))
        XCTAssertFalse(session.config.virtioNet, "The experimental default is unchanged by a view-local toggle")
        XCTAssertTrue(library.vms.isEmpty)
        session.connectionState = .stopped
    }
}
