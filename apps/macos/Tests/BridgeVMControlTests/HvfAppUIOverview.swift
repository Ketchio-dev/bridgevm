import Foundation
@testable import BridgeVMControl

/// Leaving a local draft shows the seeded list without selecting a VM.
@MainActor
enum HvfAppUIOverview {
    static let amber = "Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF"
    static let indigo = "Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF"
    static func reached(_ library: LibraryModel) -> Bool {
        !library.proMode && library.selectedID == nil
    }
}
