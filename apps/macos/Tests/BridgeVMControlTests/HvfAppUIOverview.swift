import Foundation
@testable import BridgeVMControl

/// The XCTest tier's copy of the host overview spec; AppUIHostOverviewState and
/// AppUIDriverOverviewRows are compiled only into the BRIDGEVM_APP_UI_HOST build.
/// The sidebar overview button leaves Pro mode and keeps the first-run import
/// selection, so only the content column lists the seeded rows, each once, by
/// LibraryWorkspaceRow's "<name>, <state>, <cpu>  ·  <memory>  ·  <engine>" label.
@MainActor
enum HvfAppUIOverview {
    static let amber = "Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF"
    static let indigo = "Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF"
    static func reached(_ library: LibraryModel) -> Bool {
        !library.proMode && library.selectedID == LibraryModel.firstRunImportSelectionID
    }
}
