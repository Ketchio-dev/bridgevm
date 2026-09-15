#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Darwin
import SwiftUI

@MainActor
final class AppUIHostLifecycle {
    enum Event: String, CaseIterable {
        case appInitialization = "app_initialization"
        case libraryFactory = "library_factory"
        case delegateDidFinishLaunching = "delegate_did_finish_launching"
        case representableMake = "representable_make"
        case representableUpdate = "representable_update"
        case attachmentNil = "attachment_nil"
        case attachmentNonnull = "attachment_nonnull"
    }
    private let capture: AppUIHostCapture
    private let began = ProcessInfo.processInfo.systemUptime
    private var deadline: Double?
    private var counts = Dictionary(uniqueKeysWithValues: Event.allCases.map { ($0.rawValue, 0) })
    private var firstEvents: [String: Double] = [:]
    private var firstFlags: [String: Double] = [:]
    private var flags = ["launch_finished": false, "started": false, "finished": false,
                         "window_exists": false, "window_visible": false, "window_has_content": false]
    private var observed: Double
    private var terminal = false
    private var saturated = false

    init(capture: AppUIHostCapture) throws {
        self.capture = capture
        observed = began
        try save()
    }
    static func libraryState() -> StateObject<LibraryModel> {
        AppUIHost.prepared?.lifecycle.record(.appInitialization)
        return StateObject(wrappedValue: libraryModel())
    }
    private static func libraryModel() -> LibraryModel {
        guard let host = AppUIHost.prepared else { fatalError("Diagnostic host was not prepared") }
        host.lifecycle.record(.libraryFactory)
        return host.libraryForApplication()
    }
    static func makeAttachmentView() -> NSView {
        AppUIHost.prepared?.lifecycle.record(.representableMake)
        return AppUIHostWindow.AttachmentView(frame: .zero)
    }
    func setStartupDeadline(_ deadline: Double) {
        guard self.deadline == nil, !terminal else { return }
        self.deadline = deadline
        persist()
    }
    func record(_ event: Event) {
        guard !terminal else { return }
        let key = event.rawValue
        let previous = counts[key] ?? 0
        if previous == 65_535 { saturated = true } else { counts[key] = previous + 1 }
        if firstEvents[key] == nil {
            firstEvents[key] = ProcessInfo.processInfo.systemUptime
            persist()
        }
    }
    func observe(launchFinished: Bool, started: Bool, finished: Bool, window: NSWindow?, terminal: Bool) {
        guard !self.terminal else { return }
        observed = ProcessInfo.processInfo.systemUptime
        flags = ["launch_finished": launchFinished, "started": started, "finished": finished,
                 "window_exists": window != nil, "window_visible": window?.isVisible ?? false,
                 "window_has_content": window?.contentView != nil]
        var firstTransition = false
        for (key, value) in flags where value && firstFlags[key] == nil {
            firstFlags[key] = observed
            firstTransition = true
        }
        self.terminal = terminal
        if terminal || firstTransition { persist() }
    }
    private func persist() {
        do { try save() }
        catch { capture.failed(AppUIHostError.refused("Private lifecycle receipt could not be saved")) }
    }
    private func save() throws {
        let events = Dictionary(uniqueKeysWithValues: counts.keys.map {
            ($0, firstEvents[$0].map { $0 as Any } ?? NSNull())
        })
        let transitions = Dictionary(uniqueKeysWithValues: flags.keys.map {
            ($0, firstFlags[$0].map { $0 as Any } ?? NSNull())
        })
        try capture.write(["schema_version": 1, "kind": "native-app-ui-host-lifecycle", "pid": Int(getpid()),
            "began_uptime": began, "startup_deadline_uptime": deadline.map { $0 as Any } ?? NSNull(),
            "event_counts": counts, "event_first_uptime": events, "counts_saturated": saturated,
            "coordinator": flags, "coordinator_first_true_uptime": transitions,
            "coordinator_observed_uptime": observed, "terminal": terminal], name: "host-lifecycle.json")
    }
}
#endif
