#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

extension AppUIHostWindow {
    @MainActor static func setPresentation(_ window: NSWindow, dark: Bool, minimum: Bool) async throws {
        guard let content = window.contentView, window.isVisible else {
            throw AppUIHostError.refused("Actual app window has no visible owned content")
        }
        guard let application = NSApp, let appearance = NSAppearance(named: dark ? .darkAqua : .aqua) else {
            throw AppUIHostError.refused("Requested presentation requires an existing app and named appearance")
        }
        application.appearance = appearance
        window.appearance = appearance
        window.setContentSize(NSSize(width: minimum ? 1100 : 1320, height: minimum ? 720 : 860))
        try await Task.sleep(nanoseconds: 200_000_000)
        content.layoutSubtreeIfNeeded()
        guard window.contentView === content,
              abs(content.bounds.width - (minimum ? 1100 : 1320)) < 1,
              abs(content.bounds.height - (minimum ? 720 : 860)) < 1,
              content.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == (dark ? .darkAqua : .aqua)
        else {
            AppUIHostPresentationObservation.observePresentationFailure(window, content: content, dark: dark, minimum: minimum)
            throw AppUIHostError.refused("Actual content did not adopt requested dimensions and appearance")
        }
    }
}
#endif
