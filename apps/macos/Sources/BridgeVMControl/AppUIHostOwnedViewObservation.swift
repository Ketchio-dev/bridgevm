#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostOwnedViewObservation {
    typealias Stage = AppUIHostOwnedViewProjection.Stage

    static func saveTimeout(_ host: AppUIHost, window: NSWindow, content: NSView) throws {
        try AppUIHostAXObservation.save(host, window: window, content: content)
        try save(host, window: window, content: content, stage: .welcomeTimeout)
    }

    static func saveDarkDefault(_ host: AppUIHost, window: NSWindow, content: NSView) throws {
        try save(host, window: window, content: content, stage: .darkDefault)
    }

    private static func save(_ host: AppUIHost, window: NSWindow, content: NSView, stage: Stage) throws {
        func admit() throws {
            try Task.checkCancellation()
            try AppUIHost.checkCancellation(output: host.capture.output)
            guard AppUIHost.prepared === host, window.isVisible,
                  window.contentView === content, content.window === window else {
                throw AppUIHostError.refused("Owned view observation lost visible original content")
            }
        }
        try admit()
        let tree = AppUIHostOwnedViewProjection.collect(content, stage: stage,
            owned: { $0.window === window }, fields: { AppUIHostOwnedViewFields.common($0) },
            children: { $0.subviews }, accessibility: { view, budget in
                AppUIHostOwnedViewFields.accessibility(view, owner: window, budget: &budget)
            })
        try admit()
        try write(["schema_version": 1, "kind": "native-app-ui-host-owned-views", "stage": stage.rawValue,
            "pid": Int(ProcessInfo.processInfo.processIdentifier),
            "observed_uptime": ProcessInfo.processInfo.systemUptime,
            "scope": "exact-owned-content-descendants", "record_time_nonatomic": true,
            "text_and_values_recorded": false,
            "ownership": ["prepared_host_matches": AppUIHost.prepared === host,
                          "window_visible": window.isVisible, "current_content_is_original": window.contentView === content,
                          "original_attached_to_window": content.window === window],
            "view_tree": tree], stage: stage, capture: host.capture)
    }

    static func write(_ record: [String: Any], stage: Stage, capture: AppUIHostCapture) throws {
        try capture.write(record, name: stage.filename, final: true)
    }
}
#endif
