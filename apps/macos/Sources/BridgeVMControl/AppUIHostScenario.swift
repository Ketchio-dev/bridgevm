#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
@MainActor
enum AppUIHostScenario {
    static func run(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        if let fixture = host.installCancel { return try await AppUIInstallCancelScenario.run(host, fixture: fixture, window: window, content: content) }
        if try AppUIHostDriverScenario.requested() {
            try await AppUIHostDriverScenario.run(host, window: window, content: content)
        } else {
            try await AppUIHostLegacyScenario.run(host, window: window, content: content)
        }
    }
}
#endif
