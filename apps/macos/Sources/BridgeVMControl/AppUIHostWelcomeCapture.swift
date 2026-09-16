#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostWelcomeCapture {
    static func capture(_ variant: AppUIHostPresentationMatrix.Variant, host: AppUIHost,
                        window: NSWindow, content: NSView) async throws {
        try host.capture.capture(content, name: variant.name)
        guard variant.dark && !variant.minimum else { return }
        try AppUIHostOwnedViewObservation.saveDarkDefault(host, window: window, content: content)
        try await AppUIHostWindowServerCapture.captureDarkDefault(host, window: window, content: content)
    }
}
#endif
