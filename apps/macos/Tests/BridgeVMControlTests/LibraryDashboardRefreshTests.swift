import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardRefreshTests: XCTestCase {
    /// Deleting (or moving away) the selected VM jumped the selection to the first VM in the list.
    func testAVMThatGoesAwayLeavesTheSelectionOnItsNeighbour() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let (a, b, c) = (fixture.config("a"), fixture.config("b"), fixture.config("c"))
        for config in [a, b, c] { fixture.save(config) }
        let library = fixture.library()
        XCTAssertEqual(library.vms.map(\.slug), ["a", "b", "c"])
        library.selectedID = "b"
        try fixture.removeRegistration(b); library.reload()
        XCTAssertEqual(library.selectedID, "c", "the VM after it")
        try fixture.removeRegistration(c); library.reload()
        XCTAssertEqual(library.selectedID, "a", "the last VM's: the one before it")
    }

    /// More Actions > Refresh Library has the name of the palette command that messages point to,
    /// so it reads the saved VMs again, not only the VM process.
    func testMoreActionsRefreshReadsTheLibraryAgainInBothActionBars() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let config = try XCTUnwrap(f.library.vms.first)
        let model = try XCTUnwrap(f.library.selectedModel)
        let engine = HvfEngineConfig(targetDiskPath: "disk", uefiVarsPath: "vars", evidenceDir: "unused",
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: "unused/agent.ctl")
        let session = HvfEngineSession(config: engine, repoRoot: f.root) { _ in false }
        let bars = [
            ("running controls", LibraryDashboardRuntimeControls(config: config, model: model, library: f.library,
                session: session, showAdvanced: {}).body as Any),
            ("unavailable controls", LibraryDashboardUnavailableRuntimeControls(config: config, model: model,
                library: f.library, showAdvanced: {}).body as Any),
        ]
        for (name, body) in bars {
            let bar = try XCTUnwrap(f.values(LibraryDashboardActionBar.self, in: body).first, name)
            var saved = config
            saved.name = "Changed on disk (\(name))"
            XCTAssertTrue(VMLibrary.save(saved, rootURL: f.root), name)
            bar.refresh()
            XCTAssertEqual(f.library.vms.map(\.name), [saved.name], name)
            XCTAssertEqual(f.library.selectedID, config.slug, name)
        }
    }
}
