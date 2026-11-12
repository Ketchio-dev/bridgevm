import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfVMSettingsDraftTests: XCTestCase {
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
