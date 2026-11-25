import XCTest
@testable import BridgeVMProductE2E

final class T17GuestJourneySnapshotTests: XCTestCase {
    private let sheetRoute = ["press bridgevm.dashboard.back", "press bridgevm.runtime.settings.open", "wait bridgevm.runtime.settings",
                             "press bridgevm.runtime.settings.snapshots"]
    private let backToRuntime = ["press bridgevm.runtime.settings.done", "press bridgevm.dashboard.advanced",
                                 "wait bridgevm.windows.runtime.view", "press bridgevm.windows.runtime.start"]

    func testCreatesTheSnapshotInTheSettingsSheetAndClosesItBeforeStart() throws {
        let ui = T17SettingsSheetProductUI()
        try journey(ui).createSnapshot()
        try ui.press("bridgevm.windows.runtime.start", timeout: 30)
        XCTAssertEqual(ui.events, sheetRoute + [
            "press bridgevm.runtime.snapshot.create", "text bridgevm.runtime.snapshot.status",
        ] + backToRuntime)
    }

    func testRestoresTheSnapshotThroughItsConfirmationInTheSettingsSheet() throws {
        let ui = T17SettingsSheetProductUI()
        try journey(ui).restoreSnapshot()
        try ui.press("bridgevm.windows.runtime.start", timeout: 30)
        XCTAssertEqual(ui.events, sheetRoute + [
            "press bridgevm.runtime.snapshot.restore", "press bridgevm.runtime.snapshot.confirm",
            "text bridgevm.runtime.snapshot.status",
        ] + backToRuntime)
    }

    private func journey(_ ui: T17UIControlling) -> T17GuestJourney {
        T17GuestJourney(request: Request(), ui: ui, fileManager: .default, tpm: T17TPMEvidenceLog())
    }
}

private struct Request: T17JourneyRequest {
    var jobID: String { "t17-snapshot" }; var commit: String { String(repeating: "0", count: 40) }
    var lane: Int { 1 }; var nonce: String { String(repeating: "5c", count: 32) }; var vmSlug: String { "t17-snapshot" }
    var libraryRootPath: String { "/nonexistent/library" }; var sharePath: String { "/nonexistent/share" }
    var diskPath: String { "/nonexistent/bundle/disks/hvf-target.raw" }; var varsPath: String { "/nonexistent/bundle/metadata/hvf-vars.fd" }
    var snapshotPath: String { "/nonexistent/bundle/metadata/snapshots/latest.snapshot" }
    var guestEvidencePath: String { "/nonexistent/bundle/metadata/product-e2e-guest-evidence.json" }
    var bundlePath: String { "/nonexistent/bundle" }
}
