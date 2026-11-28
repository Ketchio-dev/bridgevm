import Foundation
@testable import BridgeVMControl

/// Leaving a local draft shows the seeded list without selecting a VM.
@MainActor
enum HvfAppUIOverview {
    static let amber = "Amber, Stopped, 4 CPUs · 4 GB memory"
    static let indigo = "Indigo, Stopped, 4 CPUs · 8 GB memory"
    static func reached(_ library: LibraryModel) -> Bool {
        !library.proMode && library.selectedID == nil
    }
}
