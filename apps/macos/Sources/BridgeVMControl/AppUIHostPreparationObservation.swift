#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Foundation

@MainActor
enum AppUIHostPreparationObservation {
    static func snapshot() -> [String: Any] {
        let application = NSApp
        let applicationClass = application.map { String(reflecting: type(of: $0)) }
        let delegateClass = application?.delegate.map { String(reflecting: type(of: $0)) }
        return [
            "observed_uptime": ProcessInfo.processInfo.systemUptime, "main_thread": Thread.isMainThread,
            "application_exists": application != nil,
            "application_class": applicationClass.map { String($0.prefix(128)) as Any } ?? NSNull(),
            "delegate_class": delegateClass.map { String($0.prefix(128)) as Any } ?? NSNull(),
            "running": application.map { $0.isRunning as Any } ?? NSNull(),
            "active": application.map { $0.isActive as Any } ?? NSNull(),
            "hidden": application.map { $0.isHidden as Any } ?? NSNull()
        ]
    }

    static func write(_ capture: AppUIHostCapture, request: AppUIHostRequest,
                      environment: [String: String], before: [String: Any]) throws {
        let after = snapshot()
        let keys = ["XCTestConfigurationFilePath", "XCTestBundlePath", "XCTestSessionIdentifier",
                    "XCInjectBundle", "XCInjectBundleInto", "XCODE_RUNNING_FOR_PREVIEWS"]
        let presence = Dictionary(uniqueKeysWithValues: keys.map { ($0, environment[$0] != nil) })
        try capture.write([
            "schema_version": 1, "kind": "native-app-ui-host-preparation",
            "pid": Int(ProcessInfo.processInfo.processIdentifier), "transport": request.transport.rawValue,
            "custom_argument_count": request.argumentCount, "test_environment_present": presence,
            "before": before, "after": after
        ], name: "host-preparation.json", final: true)
    }
}
#endif
