#if BRIDGEVM_APP_UI_LAUNCH_CONTROL && (!DEBUG || !BRIDGEVM_APP_UI_HOST)
#error("BRIDGEVM_APP_UI_LAUNCH_CONTROL requires DEBUG and BRIDGEVM_APP_UI_HOST")
#endif
#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
enum AppUIHostLaunchControl {
    #if BRIDGEVM_APP_UI_LAUNCH_CONTROL
    private static var recordedMode = false
    #endif

    static func recordBody() {
        guard let host = AppUIHost.prepared else { return }
        host.lifecycle.record(.appBodyEvaluated)
        #if BRIDGEVM_APP_UI_LAUNCH_CONTROL
        guard !recordedMode else { return }
        recordedMode = true
        do {
            try host.capture.write([
                "schema_version": 1, "kind": "native-app-ui-launch-control",
                "pid": Int(ProcessInfo.processInfo.processIdentifier), "control_only": true,
                "product_ui_scenario": false, "content": "minimal-text"
            ], name: "host-launch-control.json", final: true)
        } catch {
            host.capture.failed(AppUIHostError.refused("Control-only mode receipt could not be saved"))
        }
        #endif
    }

    static func capture(_ host: AppUIHost) throws -> AppUIHostCapture {
        #if BRIDGEVM_APP_UI_LAUNCH_CONTROL
        throw AppUIHostError.refused("Control-only minimal content reached the owned ready window; product UI scenario was not run")
        #else
        return host.capture
        #endif
    }
}
#endif
