import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfVMSettingsDraftTests: XCTestCase {
    func testRenamePersistsOnTheExistingRegistrationAndPreservesLaunchIdentity() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let original = fixture.config("windows-work"), other = fixture.config("windows-personal")
        fixture.save(original)
        fixture.save(other)
        let library = fixture.library(probe: HvfClipboardSyncProbe())
        library.selectedID = original.slug
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: original))
        let before = session.config
        let display = HvfDisplayWindowController(session: session, title: original.name)
        defer { display.window?.close() }
        let draft = HvfVMSettingsDraft(session: session)
        let untouchedDraft = HvfVMSettingsDraft(session: session)
        let editedDraft = HvfVMSettingsDraft(session: session)
        editedDraft.name = other.name
        // This is a display label. It can exceed the byte budget for a new directory name.
        let newName = String(repeating: "한", count: 75)
        draft.name = "  \(newName)  "
        XCTAssertTrue(draft.finish())
        XCTAssertTrue(untouchedDraft.finish(), "Closing another window must not revert the name")
        var expected = original
        expected.name = newName
        expected.displayName = newName
        let reopened = try NativeLibraryReader.readConfig(rootURL: fixture.root, id: original.slug)
        XCTAssertEqual(reopened, expected)
        XCTAssertEqual(library.latestConfiguration(for: original), expected)
        XCTAssertEqual(library.selectedID, original.slug)
        XCTAssertTrue(library.hvfRuntimeSessions.owns(session, for: expected))
        XCTAssertEqual(session.config.libraryContext?.config, expected)
        XCTAssertEqual(display.window?.title, newName)
        XCTAssertEqual(session.config.vtpmKeyID, before.vtpmKeyID)
        XCTAssertEqual(session.config.vtpmStateDir, before.vtpmStateDir)
        XCTAssertEqual(session.config.targetDiskPath, before.targetDiskPath)
        XCTAssertEqual(session.config.uefiVarsPath, before.uefiVarsPath)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.root, id: other.slug), other)
        XCTAssertEqual(untouchedDraft.name, newName)
        XCTAssertFalse(editedDraft.finish(), "Closing a stale draft must not overwrite the other window")
        XCTAssertTrue(try XCTUnwrap(editedDraft.nameMessage).contains(newName))
        XCTAssertEqual(editedDraft.name, other.name)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.root, id: original.slug), expected)
        XCTAssertTrue(editedDraft.finish(), "The user can explicitly use the same name as another VM")
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.root, id: original.slug).name, other.name)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.root, id: other.slug), other)
    }

    func testInvalidAndBusyRenameKeepTheSavedName() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let original = fixture.config()
        fixture.save(original)
        let library = fixture.library(probe: HvfClipboardSyncProbe())
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: original))
        let draft = HvfVMSettingsDraft(session: session)
        draft.name = " "
        XCTAssertFalse(draft.finish())
        XCTAssertNotNil(draft.nameMessage)
        session.connectionState = .booting
        defer { session.connectionState = .stopped }
        draft.name = "새 이름"
        XCTAssertFalse(draft.finish())
        XCTAssertNotNil(draft.nameMessage)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.root, id: original.slug), original)
    }

    func testDoneSavesTypedPathsAndClearedForwardsWithoutReturn() throws {
        let f = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { f.clean() }
        var saved = f.config(); saved.portForwards = "8080:80"; f.save(saved)
        let library = f.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let draft = HvfVMSettingsDraft(session: session)
        draft.shareEnabled = true; draft.host = f.root.path; draft.guest = "C:\\shared"; draft.forwards = ""
        XCTAssertTrue(draft.finish())
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: f.registration(saved)))
        XCTAssertEqual(file.sharedFolderHostPath, f.root.path)
        XCTAssertEqual(file.sharedFolderGuestPath, "C:\\shared")
        XCTAssertNil(file.portForwards)
    }

    func testInvalidTextStaysVisibleAndDoneRefusesDismissal() throws {
        let f = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { f.clean() }
        var saved = f.config(); saved.portForwards = "8080:80"; f.save(saved)
        let library = f.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let draft = HvfVMSettingsDraft(session: session)
        draft.forwards = "not-a-port"
        XCTAssertFalse(draft.finish())
        XCTAssertEqual(draft.forwards, "not-a-port")
        XCTAssertNotNil(draft.forwardsMessage)
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: f.registration(saved)))
        XCTAssertEqual(file.portForwards, "8080:80")
    }

    func testStartIsRefusedWhileSettingsSheetIsOpen() throws {
        let f = try HvfRuntimeSessionStoreFixture()
        defer { f.clean() }
        let library = f.library()
        let session = library.experimentalHvfRuntimeSession()
        session.settingsSheetOpen = true
        guard case let .refused(message) = session.requestGUIStart(configuration: session.config) else {
            return XCTFail("Start must not admit a visible settings draft")
        }
        XCTAssertEqual(message, "설정을 완료한 뒤 Windows를 시작하세요.")
    }

    func testExperimentalTextCommitsReachLaunchConfiguration() throws {
        let f = try HvfRuntimeSessionStoreFixture()
        defer { f.clean() }
        let library = f.library()
        let session = library.experimentalHvfRuntimeSession()
        let draft = HvfVMSettingsDraft(session: session)
        draft.shareEnabled = true; draft.host = f.root.path; draft.guest = "C:\\shared"
        draft.forwards = "8080:80"
        XCTAssertTrue(draft.commitShare())
        XCTAssertTrue(draft.commitForwards())
        XCTAssertEqual(session.config.shareHostDir, f.root.path)
        XCTAssertEqual(session.config.shareGuestDir, "C:\\shared")
        XCTAssertEqual(session.config.portForwards, [HvfPortForward(host: 8080, guest: 80)])
    }
}
