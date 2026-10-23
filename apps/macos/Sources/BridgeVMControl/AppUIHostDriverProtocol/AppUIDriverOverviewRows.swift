#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

/// The two seeded overview rows by their exact current accessibility label,
/// LibraryWorkspaceRow's "<name>, <state>, <cpu>  ·  <memory>  ·  <engine>".
/// Fixture models are inert and never running, so the state is "정지됨".
/// Each label must match exactly one button; a second list is ambiguous.
enum AppUIDriverOverviewRows {
    static let amber = "Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF"
    static let indigo = "Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF"
}
#endif
