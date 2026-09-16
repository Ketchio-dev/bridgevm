#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostScenario {
    static func run(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        if try AppUIHostDriverScenario.requested() {
            try await AppUIHostDriverScenario.run(host, window: window, content: content)
        } else {
            try await AppUIHostLegacyScenario.run(host, window: window, content: content)
        }
    }
}
#endif
