import Foundation

/// Clipboard, network, shared folder and snapshots are in the VM's Settings sheet, opened from the
/// dashboard; Start and the guest control input stay in the runtime view behind Advanced.
/// Start is refused while the sheet is open, so the sheet is gone before the runtime view returns.
enum T17SettingsSheet {
    static let sheet = "bridgevm.runtime.settings"
    static let done = "bridgevm.runtime.settings.done"
    static let sharing = "bridgevm.runtime.settings.sharing"
    static let network = "bridgevm.runtime.settings.network"
    static let snapshots = "bridgevm.runtime.settings.snapshots"

    /// From the runtime view, runs `steps` in the sheet and comes back to the runtime view.
    static func edit(_ ui: T17UIControlling, section: String, _ steps: () throws -> Void) throws {
        try ui.press("bridgevm.dashboard.back", timeout: 10)
        try ui.press("bridgevm.runtime.settings.open", timeout: 10)
        try ui.waitFor(sheet, timeout: 10)
        try ui.press(section, timeout: 10)
        try steps()
        try ui.press(done, timeout: 10)
        let deadline = Date().addingTimeInterval(10)
        while (try? ui.optionalTexts([done]))?.isEmpty != true {
            guard Date() < deadline else {
                throw T17Blocker(code: "ui-element-missing", detail: "settings sheet did not close: \(sheet)")
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
        try ui.press("bridgevm.dashboard.advanced", timeout: 10)
        try ui.waitFor("bridgevm.windows.runtime.view", timeout: 60)
    }
}
