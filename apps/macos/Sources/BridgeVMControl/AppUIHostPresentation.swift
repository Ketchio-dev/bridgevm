#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

extension AppUIHostWindow {
    @MainActor static func setPresentation(_ window: NSWindow, dark: Bool, minimum: Bool) async throws {
        guard let content = window.contentView, window.isVisible else {
            throw AppUIHostError.refused("Actual app window has no visible owned content")
        }
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.setContentSize(NSSize(width: minimum ? 1100 : 1320, height: minimum ? 720 : 860))
        try await Task.sleep(nanoseconds: 200_000_000)
        content.layoutSubtreeIfNeeded()
        guard window.contentView === content,
              abs(content.bounds.width - (minimum ? 1100 : 1320)) < 1,
              abs(content.bounds.height - (minimum ? 720 : 860)) < 1,
              content.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == (dark ? .darkAqua : .aqua)
        else {
            observePresentationFailure(window, content: content, dark: dark, minimum: minimum)
            throw AppUIHostError.refused("Actual content did not adopt requested dimensions and appearance")
        }
    }

    private static func observePresentationFailure(_ window: NSWindow, content: NSView,
                                                   dark: Bool, minimum: Bool) {
        do {
            guard let capture = AppUIHost.prepared?.capture else {
                throw AppUIHostError.refused("Presentation observation has no prepared host")
            }
            let current = window.contentView
            try capture.write([
                "schema_version": 1, "kind": "native-app-ui-host-presentation-failure",
                "pid": Int(ProcessInfo.processInfo.processIdentifier),
                "observed_uptime": ProcessInfo.processInfo.systemUptime,
                "requested_dark": dark, "requested_minimum": minimum,
                "requested_content_size_points": ["width": minimum ? 1100 : 1320,
                                                  "height": minimum ? 720 : 860],
                "current_content_is_original": current === content,
                "original_content": presentationContent(content, window: window),
                "current_content": presentationContent(current, window: window),
                "window_visible": window.isVisible,
                "window_frame_screen_points": presentationRect(window.frame),
                "window_content_layout_rect_window_points": presentationRect(window.contentLayoutRect),
                "window_style_mask": window.styleMask.rawValue,
                "window_backing_scale_factor": Double(window.backingScaleFactor),
                "window_explicit_appearance": presentationName(window.appearance?.name),
                "window_effective_appearance": presentationName(window.effectiveAppearance.name),
                "window_best_match": presentationName(window.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]))
            ], name: "host-presentation-failure.json", final: true)
        } catch {
            FileHandle.standardError.write(Data("App UI presentation observation could not be saved.\n".utf8))
        }
    }

    private static func presentationContent(_ view: NSView?, window: NSWindow) -> [String: Any] {
        ["exists": view != nil, "attached_to_requested_window": view?.window === window,
         "bounds_local_points": view.map { presentationRect($0.bounds) as Any } ?? NSNull(),
         "frame_superview_points": view.map { presentationRect($0.frame) as Any } ?? NSNull(),
         "explicit_appearance": presentationName(view?.appearance?.name),
         "effective_appearance": presentationName(view?.effectiveAppearance.name),
         "best_match": presentationName(view?.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]))]
    }

    private static func presentationRect(_ rect: NSRect) -> [String: Double] {
        ["x": Double(rect.origin.x), "y": Double(rect.origin.y),
         "width": Double(rect.width), "height": Double(rect.height)]
    }

    private static func presentationName(_ name: NSAppearance.Name?) -> Any {
        name.map { String($0.rawValue.prefix(128)) as Any } ?? NSNull()
    }
}
#endif
