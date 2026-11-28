#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

/// The two seeded overview rows by their exact current accessibility label,
/// LibraryWorkspaceRow's "<name>, <state>, <cpu> · <memory>" (an unsupported engine adds " · Unsupported engine").
/// Fixture models are inert and never running, so the state is "Stopped".
/// Each label must match exactly one button; a second list is ambiguous.
enum AppUIDriverOverviewRows {
    static let amber = "Amber, Stopped, 4 CPUs · 4 GB memory"
    static let indigo = "Indigo, Stopped, 4 CPUs · 8 GB memory"
}
#endif
