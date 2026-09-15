#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostWelcomeMatrix {
    static func run(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        let capture = host.capture
        try await AppUIHostPresentationMatrix.run(
            admission: {
                try AppUIHost.checkCancellation(output: capture.output)
                guard AppUIHost.prepared === host, window.isVisible,
                      window.contentView === content, content.window === window else {
                    throw AppUIHostError.refused("Presentation matrix lost visible owned original content")
                }
            },
            present: { variant in
                try await AppUIHostWindow.setPresentation(window, dark: variant.dark, minimum: variant.minimum)
            },
            capture: { variant in try capture.capture(content, name: variant.name) },
            mismatch: { error in
                capture.failed(error)
                try capture.save()
            },
            observe: { observation(window, content: content) },
            persist: { rows in
                try capture.write([
                    "schema_version": 1, "kind": "native-app-ui-host-presentation-matrix",
                    "pid": Int(ProcessInfo.processInfo.processIdentifier),
                    "rows": rows.map { row in
                        ["name": row.variant.name, "requested_dark": row.variant.dark,
                         "requested_minimum": row.variant.minimum,
                         "requested_content_size_points": ["width": row.variant.minimum ? 1100 : 1320,
                                                           "height": row.variant.minimum ? 720 : 860],
                         "outcome": row.outcome.rawValue, "observed": row.observation] as [String: Any]
                    }
                ], name: "host-presentation-matrix.json")
            })
    }

    private static func observation(_ window: NSWindow, content: NSView) -> [String: Any] {
        let bounds = AppUIHostGeometryObservation.presentationRect(content.bounds).mapValues {
            $0.isFinite ? $0 as Any : NSNull()
        }
        return [
            "observed_uptime": ProcessInfo.processInfo.systemUptime,
            "original_content_bounds_local_points": bounds,
            "window_visible": window.isVisible, "current_content_exists": window.contentView != nil,
            "current_content_is_original": window.contentView === content,
            "original_attached_to_window": content.window === window,
            "original_explicit_appearance": name(content.appearance?.name),
            "original_effective_appearance": name(content.effectiveAppearance.name),
            "original_best_match": name(content.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]))
        ]
    }

    private static func name(_ appearance: NSAppearance.Name?) -> Any {
        appearance.map { String($0.rawValue.prefix(128)) as Any } ?? NSNull()
    }
}
#endif
