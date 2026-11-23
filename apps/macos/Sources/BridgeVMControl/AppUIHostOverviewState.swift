#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

/// The state the sidebar overview button produces from the import form.
/// LibrarySidebar.showOverview only leaves Pro mode and keeps a non-nil
/// selection, so the import selection stays, the detail pane keeps the import
/// form and only the content column lists the seeded rows.
@MainActor
enum AppUIHostOverviewState {
    static func reached(_ library: LibraryModel) -> Bool {
        !library.proMode && library.selectedID == LibraryModel.firstRunImportSelectionID
    }
}
#endif
