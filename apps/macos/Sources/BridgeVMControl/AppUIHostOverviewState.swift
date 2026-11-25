#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

/// Leaving a local draft shows the seeded list without selecting a VM.
@MainActor
enum AppUIHostOverviewState {
    static func reached(_ library: LibraryModel) -> Bool {
        !library.proMode && library.selectedID == nil
    }
}
#endif
