import SwiftUI
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
        XCTAssertEqual(editedDraft.nameMessage, "Another window saved the name “\(newName)”. Click Done again to use your edit.")
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
        XCTAssertEqual(draft.nameMessage, "Enter a VM name with 1 to 128 characters and no control characters.")
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

    func testInvalidPortForwardKeepsSettingsOpenOnlyWhileNetworkingIsOn() throws {
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
        XCTAssertNil(session.applyNetworkToggle(false).message)
        XCTAssertTrue(draft.finish(), "Offline settings can close with the port field hidden")
        let file = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: f.registration(saved)))
        XCTAssertEqual(file.networkEnabled, false)
        XCTAssertEqual(file.portForwards, "8080:80")
        let reopened = HvfVMSettingsDraft(session: session)
        XCTAssertEqual(reopened.forwards, "8080:80")
    }

    func testStartWaitsForTheLastSettingsSheetToClose() throws {
        let f = try HvfRuntimeSessionStoreFixture()
        defer { f.clean() }
        let saved = f.config()
        f.save(saved)
        let library = f.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let dashboard = HvfVMSettingsDraft(session: session)
        let display = HvfVMSettingsDraft(session: session)
        dashboard.didAppear()
        display.didAppear()
        dashboard.didDisappear()
        guard case let .refused(message) = session.requestGUIStart(configuration: session.config) else {
            return XCTFail("Start must not admit a visible settings draft")
        }
        XCTAssertEqual(message, "Close Settings before starting Windows.")
        display.didDisappear()
        XCTAssertTrue(session.openSettingsSheets.isEmpty)
    }

    /// The sheet also closes without Done, for example when a finished Duplicate selects the copy.
    /// Before: the typed name was dropped, and nothing told the user.
    func testClosingWithoutDoneSavesTheTypedNameAndReportsWhatWasNotSaved() throws {
        let f = try HvfRuntimeSessionStoreFixture()
        defer { f.clean() }
        let saved = f.config(); f.save(saved)
        let library = f.library(probe: HvfClipboardSyncProbe())
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let renamed = HvfVMSettingsDraft(session: session)
        renamed.didAppear()
        renamed.name = "Work PC"
        renamed.didDisappear()
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.root, id: saved.slug).name, "Work PC")
        XCTAssertNil(library.operationError)

        let invalid = HvfVMSettingsDraft(session: session)
        invalid.name = " "
        invalid.didDisappear()
        XCTAssertEqual(library.operationError, "Enter a VM name with 1 to 128 characters and no control characters.")
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.root, id: saved.slug).name, "Work PC")
    }

    /// ⌘Q with Settings open: the typed name and port forwards were dropped (they save on Return, Done,
    /// or a focus change, and the sheet's onDisappear doesn't run at quit).
    func testQuitSavesWhatOpenSettingsSheetsHold() throws {
        let f = try HvfRuntimeSessionStoreFixture()
        defer { f.clean() }
        let saved = f.config(); f.save(saved)
        let library = f.library(probe: HvfClipboardSyncProbe())
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let sheet = HvfVMSettingsDraft(session: session)
        sheet.didAppear()
        sheet.name = "Work PC"
        sheet.forwards = "8080:80"
        HvfVMSettingsDraft.saveOpenSheets()
        let file = try NativeLibraryReader.readConfig(rootURL: f.root, id: saved.slug)
        XCTAssertEqual(file.name, "Work PC")
        XCTAssertEqual(file.portForwards, "8080:80")
        sheet.didDisappear()
    }

    /// Turning sharing on only reveals the fields. Before: it saved at once with an empty Mac folder,
    /// which showed a warning before the user could type anything.
    func testTurningSharingOnWithoutAFolderWaitsForDone() throws {
        let f = try HvfRuntimeSessionStoreFixture()
        defer { f.clean() }
        let saved = f.config(); f.save(saved)
        let original = try Data(contentsOf: f.registration(saved))
        let library = f.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let draft = HvfVMSettingsDraft(session: session)
        let settings = HvfSharedFolderSettings(session: session,
            enabled: Binding(get: { draft.shareEnabled }, set: { draft.shareEnabled = $0 }),
            hostDir: Binding(get: { draft.host }, set: { draft.host = $0 }),
            guestDir: Binding(get: { draft.guest }, set: { draft.guest = $0 }),
            commit: { draft.commitShare() })
        settings.setEnabled(true)
        XCTAssertTrue(draft.shareEnabled)
        XCTAssertNil(draft.shareMessage)
        XCTAssertEqual(try Data(contentsOf: f.registration(saved)), original)
        XCTAssertFalse(draft.finish(), "Done still asks for the Mac folder")
        XCTAssertEqual(draft.shareMessage, "Enter the full path to a Mac folder and a destination path in Windows.")
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
