import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardRefreshTests: XCTestCase {
    /// Several messages say "Refresh the library"; More Actions > Refresh is the button they mean,
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
