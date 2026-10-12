import XCTest
@testable import BridgeVMControl

/// Records every disk-path process probe a runtime session makes; never runs pgrep.
final class HvfClipboardSyncProbe {
    var running = false
    var lookups: [String] = []
    var sessions = 0
}

@MainActor
extension HvfRuntimeSessionStoreFixture {
    func library(probe: HvfClipboardSyncProbe) -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, runtimeSessionFactory: { [root] config in
            probe.sessions += 1
            return HvfEngineSession(config: config, repoRoot: root,
                processIsRunning: { probe.lookups.append($0); return probe.running })
        }, modelFactory: { ControlModel(config: $0, startsAutomatically: false) })
    }

    func registration(_ config: VMConfig) -> URL { root.appendingPathComponent(config.slug + "/vm.json") }
}

@MainActor
final class HvfClipboardSyncPreferenceSaveTests: XCTestCase {
    // Inspect constructed values only, as HvfRuntimeSessionStoreTests does; never render.
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func detail(_ library: LibraryModel) throws -> (view: HvfEngineView, ids: [ObjectIdentifier]) {
        let body = LibraryDetailView(library: library).body
        return (try XCTUnwrap(values(HvfEngineView.self, in: body).first), values(ObjectIdentifier.self, in: body))
    }

    private static let unsynced: (Data, URL) -> VMRegistrationCommitOutcome = {
        VMRegistrationWriter.commit($0, to: $1, syncParent: { _ in throw CocoaError(.fileWriteUnknown) })
    }

    /// Review finding 1: the save replaced the stopped session, so the `.id(ObjectIdentifier(session))`
    /// detail view was rebuilt and loadStateFromSession dropped every other unsaved edit.
    func testSavingKeepsStoppedSessionAndDetailIdentitySoUnsavedEditsSurvive() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        library.selectedID = saved.slug
        let before = try detail(library)
        let session = before.view.session
        XCTAssertTrue(before.ids.contains(ObjectIdentifier(session)))
        var edited = session.config; edited.virtioNet = false; edited.ramMiB = 8192
        XCTAssertTrue(session.acceptStartConfiguration(edited)) // options held for the next view Start

        XCTAssertNil(session.saveClipboardSyncPreference(false))
        let after = try detail(library)
        XCTAssertTrue(after.view.session === session, "The view keeps its identity and @State edits")
        XCTAssertTrue(after.ids.contains(ObjectIdentifier(session)))
        XCTAssertEqual(probe.sessions, 1)
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.clipboardSyncEnabled, false)
        XCTAssertTrue(library.hvfRuntimeSessions.owns(session, for: latest))
        XCTAssertEqual(session.config.libraryContext?.config, latest)
        XCTAssertFalse(session.config.clipboardSync)
        XCTAssertFalse(session.config.virtioNet, "An unsaved network-off choice is not reverted")
        XCTAssertEqual(session.config.ramMiB, 8192)
        XCTAssertEqual(probe.lookups, [session.config.targetDiskPath])
    }

    /// Review finding 2: a replaced-but-unsynced vm.json was reported as a failure without reload,
    /// so the app kept showing the old value while the file held the new one.
    func testUnconfirmedPublicationReloadsSoAppMatchesFile() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let shown = session.applyClipboardSyncToggle(false, publish: Self.unsynced)
        XCTAssertNotNil(shown.message, "Unconfirmed durability is reported")
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: fixture.registration(saved)))
        XCTAssertEqual(file.clipboardSyncEnabled, false)
        XCTAssertFalse(shown.isOn, "The toggle shows vm.json's value")
        XCTAssertEqual(library.latestConfiguration(for: saved).clipboardSyncEnabled, false)
        let current = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertTrue(current === session)
        XCTAssertFalse(current.config.clipboardSync)
        XCTAssertEqual(current.config.libraryContext?.config.clipboardSyncEnabled, false)
    }

    /// If the registration also changed while publishing, the in-place rebind is impossible;
    /// the view is rebuilt from vm.json and that is reported, never silent.
    func testRegistrationChangedDuringSaveIsReportedNotSilent() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertNil(session.saveClipboardSyncPreference(false, publish: { data, url in
            guard var raced = try? JSONDecoder().decode(VMConfig.self, from: data) else { return .notPublished(CocoaError(.coderReadCorrupt)) }
            raced.memMiB = 8192
            let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            return VMRegistrationWriter.commit((try? encoder.encode(raced)) ?? data, to: url)
        }))
        XCTAssertNotNil(library.operationError)
        let current = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertFalse(current === session)
        XCTAssertFalse(current.config.clipboardSync)
        XCTAssertEqual(current.config.ramMiB, 8192)
    }

    func testUnpublishedSaveLeavesFileLibraryAndSessionUnchanged() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let original = try Data(contentsOf: fixture.registration(saved))
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let shown = session.applyClipboardSyncToggle(false,
            publish: { _, _ in .notPublished(CocoaError(.fileWriteNoPermission)) })
        XCTAssertNotNil(shown.message)
        XCTAssertTrue(shown.isOn, "The toggle keeps vm.json's unchanged value")
        XCTAssertEqual(try Data(contentsOf: fixture.registration(saved)), original)
        XCTAssertNil(library.latestConfiguration(for: saved).clipboardSyncEnabled)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertTrue(session.config.clipboardSync)
    }

    /// Review finding 3: a VM process the app is not attached to left the session "stopped".
    func testUnattachedRunningProcessRefusesSave() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let original = try Data(contentsOf: fixture.registration(saved))
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        probe.running = true
        XCTAssertTrue(session.applyClipboardSyncToggle(false).isOn)
        XCTAssertEqual(probe.lookups, [session.config.targetDiskPath])
        XCTAssertEqual(try Data(contentsOf: fixture.registration(saved)), original)
        XCTAssertNil(library.latestConfiguration(for: saved).clipboardSyncEnabled)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertTrue(session.config.clipboardSync)
    }

    func testExperimentalEntryKeepsViewLocalToggle() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let library = fixture.library()
        let session = library.experimentalHvfRuntimeSession()
        XCTAssertFalse(session.persistsClipboardSyncPreference)
        session.connectionState = .booting
        XCTAssertFalse(HvfClipboardSyncToggle(session: session, isOn: .constant(true)).isDisabled)
        XCTAssertNil(session.saveClipboardSyncPreference(false))
        XCTAssertTrue(session.applyClipboardSyncToggle(false) == (false, nil))
        XCTAssertTrue(session.config.clipboardSync)
        XCTAssertTrue(library.vms.isEmpty)
        session.connectionState = .stopped
    }
}
