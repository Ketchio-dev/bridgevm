import Foundation

/// After an import the product shows the VM's dashboard, as after a T17 install
/// (d34415a5); its runtime controls sit behind 고급. The import may take a while.
enum A9ImportRuntimeNavigation {
    static let importTimeout: TimeInterval = 1_200

    static func openRuntimeControls(_ ui: T17UIControlling) throws {
        if (try? ui.waitFor("bridgevm.windows.runtime.view", timeout: 1)) == nil {
            try ui.press("bridgevm.dashboard.advanced", timeout: importTimeout)
        }
        try ui.waitFor("bridgevm.windows.runtime.view", timeout: 60)
    }
}
