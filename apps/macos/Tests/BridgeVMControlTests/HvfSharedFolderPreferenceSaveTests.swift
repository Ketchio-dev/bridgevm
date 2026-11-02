import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfSharedFolderPreferenceSaveTests: XCTestCase {
    private static let shareKeys = ["sharedFolderHostPath", "sharedFolderGuestPath"]

    private func chosenFolder(_ fixture: HvfRuntimeSessionStoreFixture) throws -> HvfSharedFolderSelection {
        let folder = fixture.root.appendingPathComponent("chosen-share", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return HvfSharedFolderSelection(hostPath: folder.path, guestPath: "C:\\bridgevm-share")
    }

    private func lines(_ data: Data) -> [Substring] {
        String(decoding: data, as: UTF8.self).split(separator: "\n").filter { line in
            !Self.shareKeys.contains { line.contains("\"\($0)\" :") }
        }
    }

    func testSavingAddsOnlyShareKeysAndStoppingRestoresOriginalBytesAndDigest() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.networkEnabled = true; fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let canonical = library.latestConfiguration(for: saved)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: canonical)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))

        XCTAssertNil(session.saveSharedFolderPreference(selection))
        let shared = try Data(contentsOf: url)
        XCTAssertEqual(lines(shared), lines(original), "Every other vm.json line is byte-for-byte unchanged")
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: shared).sharedFolder, selection)

        let stopped = session.applySharedFolderSelection(nil)
        XCTAssertNil(stopped.selection); XCTAssertNil(stopped.message)
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertEqual(library.latestConfiguration(for: saved), canonical)
        XCTAssertEqual(try NativeRuntimeConfigurationIdentity.digest(config: library.latestConfiguration(for: saved)), digest)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertNil(session.config.shareHostDir); XCTAssertNil(session.config.shareGuestDir)
        XCTAssertEqual(probe.sessions, 1)
    }

    /// Only an existing folder the user selected is saved; nothing else is substituted.
    func testMissingRelativeFileOrIncompleteSelectionIsNotSaved() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let folder = try chosenFolder(fixture)
        let file = fixture.root.appendingPathComponent("plain-file")
        try Data("not a folder".utf8).write(to: file)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        for rejected in [
            HvfSharedFolderSelection(hostPath: folder.hostPath + "-absent", guestPath: folder.guestPath),
            HvfSharedFolderSelection(hostPath: "chosen-share", guestPath: folder.guestPath),
            HvfSharedFolderSelection(hostPath: file.path, guestPath: folder.guestPath),
            HvfSharedFolderSelection(hostPath: folder.hostPath, guestPath: "  "),
        ] {
            let result = session.applySharedFolderSelection(rejected)
            XCTAssertNotNil(result.message, rejected.hostPath)
            XCTAssertNil(result.selection, "vm.json still shares nothing")
            XCTAssertEqual(try Data(contentsOf: url), original)
        }
        XCTAssertTrue(probe.lookups.isEmpty, "Refused before any save check")
        XCTAssertNil(session.config.shareHostDir)
    }

    /// Runtime-view Start saves a typed but unsaved pair first, so every later start matches it.
    func testRuntimeViewStartSavesVisibleShareBeforeLaunching() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertNil(session.saveSharedFolderBeforeStart(nil), "Nothing visible, nothing saved")
        XCTAssertEqual(try Data(contentsOf: url), original)
        let missing = HvfSharedFolderSelection(hostPath: selection.hostPath + "-absent", guestPath: selection.guestPath)
        XCTAssertNotNil(session.saveSharedFolderBeforeStart(missing), "An unsavable share refuses the start")
        XCTAssertEqual(try Data(contentsOf: url), original)

        XCTAssertNil(session.saveSharedFolderBeforeStart(selection))
        XCTAssertEqual(library.latestConfiguration(for: saved).sharedFolder, selection)
        XCTAssertEqual(session.config.shareHostDir, selection.hostPath)
        let shared = try Data(contentsOf: url)
        XCTAssertNil(session.saveSharedFolderBeforeStart(selection))
        XCTAssertEqual(try Data(contentsOf: url), shared, "An already saved share is not rewritten")
        XCTAssertEqual(probe.lookups.count, 1)
        XCTAssertNil(library.experimentalHvfRuntimeSession().saveSharedFolderBeforeStart(missing),
                     "The experimental entry keeps view-local options")
    }

    func testSettingsAreDisabledAndSaveRefusedWhileRunningOrBusy() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let settings = HvfSharedFolderSettings(session: session, enabled: .constant(true),
            hostDir: .constant(selection.hostPath), guestDir: .constant(""))
        XCTAssertFalse(settings.isDisabled)
        XCTAssertNil(settings.draft, "An empty guest path is still a draft")
        session.connectionState = .connected(host: "fixture")
        XCTAssertTrue(settings.isDisabled)
        XCTAssertNotNil(session.saveSharedFolderPreference(selection))
        XCTAssertNotNil(session.saveSharedFolderBeforeStart(selection))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertNil(library.latestConfiguration(for: saved).sharedFolder)
        session.connectionState = .stopped
    }

    /// Saving one preference rebinds in place: other unsaved session options and earlier saves survive.
    func testSequentialSavesKeepSessionAndUnsavedOptions() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        var edited = session.config; edited.ramMiB = 8192
        XCTAssertTrue(session.acceptStartConfiguration(edited))
        XCTAssertNil(session.saveSharedFolderPreference(selection))
        XCTAssertNil(session.saveNetworkPreference(false))
        XCTAssertNil(session.saveClipboardSyncPreference(false))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.sharedFolder, selection)
        XCTAssertEqual(latest.networkEnabled, false)
        XCTAssertEqual(latest.clipboardSyncEnabled, false)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertTrue(library.hvfRuntimeSessions.owns(session, for: latest))
        XCTAssertEqual(session.config.libraryContext?.config, latest)
        XCTAssertEqual(session.config.shareHostDir, selection.hostPath)
        XCTAssertFalse(session.config.virtioNet); XCTAssertFalse(session.config.clipboardSync)
        XCTAssertEqual(session.config.ramMiB, 8192, "An unsaved option is not reverted")
        XCTAssertNil(library.operationError)
        XCTAssertEqual(probe.sessions, 1)
    }

    /// A clone is a new VM: it keeps network/clipboard choices but never inherits the source's share.
    func testCloneRegistrationDropsSharedFolder() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        var template = fixture.config()
        template.networkEnabled = false; template.clipboardSyncEnabled = false
        template.sharedFolderHostPath = "/Users/source/share"; template.sharedFolderGuestPath = "C:\\bridgevm-share"
        let bundle = fixture.root.appendingPathComponent("copy/bundle.vmbridge", isDirectory: true)
        let copy = template.windowsCloneRegistration(slug: "copy", name: "Copy", bundle: bundle)
        XCTAssertNil(copy.sharedFolderHostPath); XCTAssertNil(copy.sharedFolderGuestPath)
        XCTAssertEqual(copy.networkEnabled, false); XCTAssertEqual(copy.clipboardSyncEnabled, false)
        XCTAssertEqual(copy.slug, "copy"); XCTAssertEqual(copy.bundlePath, bundle.path)
        XCTAssertEqual(copy.diskPath, bundle.appendingPathComponent("disks/hvf-target.raw").path)
        XCTAssertEqual(copy.installPending, false)
        XCTAssertNil(try XCTUnwrap(HvfEngineConfig.libraryVM(copy, rootURL: fixture.root)).shareHostDir)
    }
}
