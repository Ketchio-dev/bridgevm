import AppKit
import Darwin

@MainActor
enum ControlAppActivation {
    static func activate() {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        let before = NSApp.activationPolicy().rawValue
        let changed = NSApp.setActivationPolicy(.regular)
        let after = NSApp.activationPolicy().rawValue
        #else
        NSApp.setActivationPolicy(.regular)
        #endif
        NSApp.activate(ignoringOtherApps: true)
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        guard let capture = AppUIHost.prepared?.capture else { return }
        do {
            try capture.write([
                "pid": Int(getpid()),
                "observed_uptime": ProcessInfo.processInfo.systemUptime,
                "activation_policy_before": before,
                "policy_change_succeeded": changed,
                "activation_policy_after": after
            ], name: "host-activation.json", final: true)
        } catch {
            capture.failed(AppUIHostError.refused("Private activation receipt could not be saved"))
        }
        #endif
    }
}
