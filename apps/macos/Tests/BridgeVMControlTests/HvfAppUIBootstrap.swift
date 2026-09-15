import AppKit

@MainActor
final class HvfAppUIBootstrap: NSObject, NSApplicationDelegate {
    private static var launchAttempted = false
    private let app: NSApplication
    private let previousDelegate: (any NSApplicationDelegate)?
    private let output: URL
    private var installed = false
    private var phase = "created"
    private var before: [String: Bool] = [:]
    private var after: [String: Bool] = [:]
    private var willFinish = false
    private var didFinish = false
    private var documentRequests = 0
    private var untitledQueries = 0
    private var reopenQueries = 0

    init(output: URL) throws {
        self.output = output
        app = NSApplication.shared
        previousDelegate = app.delegate
        super.init()
        before = state()
        // Inspect only presence. Never serialize a launch path or edit defaults.
        guard UserDefaults.standard.object(forKey: "NSOpen") == nil else {
            phase = "refused-nsopen-default"
            try save()
            throw HvfAppUIError.refused("Native UI bootstrap refuses NSOpen defaults")
        }
        guard !Self.launchAttempted else {
            phase = "refused-repeated-launch"
            try save()
            throw HvfAppUIError.refused("Native UI bootstrap may finish launching only once")
        }
        Self.launchAttempted = true
        app.setActivationPolicy(.accessory)
        app.delegate = self
        installed = true
        do {
            phase = "before-finish-launching"
            try save()
            app.finishLaunching()
            after = state()
            phase = "after-finish-launching"
            try save()
            try assertNoDocumentRequests()
        } catch {
            close()
            throw error
        }
    }

    func assertNoDocumentRequests() throws {
        guard documentRequests == 0 else {
            throw HvfAppUIError.refused("Native UI bootstrap refused document opening")
        }
    }

    func close() {
        if installed { app.delegate = previousDelegate; installed = false }
        try? save()
    }

    private func state() -> [String: Bool] {
        ["running": app.isRunning, "finished_launching": NSRunningApplication.current.isFinishedLaunching]
    }

    private func save() throws {
        let record: [String: Any] = ["schema_version": 1, "kind": "owned-native-launch-bootstrap",
            "phase": phase, "before": before, "after": after,
            "will_finish_notification": willFinish, "did_finish_notification": didFinish,
            "document_requests_refused": documentRequests, "untitled_queries_refused": untitledQueries,
            "reopen_queries_refused": reopenQueries, "delegate_installed": installed]
        let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: output.appendingPathComponent("ui-bootstrap.json"), options: .atomic)
    }

    private func refuseDocument() -> Bool {
        documentRequests += 1
        try? save()
        return false
    }

    func applicationWillFinishLaunching(_ notification: Notification) { willFinish = true }
    func applicationDidFinishLaunching(_ notification: Notification) { didFinish = true }
    func applicationShouldOpenUntitledFile(_ sender: NSApplication) -> Bool {
        untitledQueries += 1
        return false
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        reopenQueries += 1
        return false
    }
    func applicationOpenUntitledFile(_ sender: NSApplication) -> Bool { refuseDocument() }
    func application(_ sender: NSApplication, openFile filename: String) -> Bool { refuseDocument() }
    func application(_ sender: Any, openFileWithoutUI filename: String) -> Bool { refuseDocument() }
    func application(_ sender: NSApplication, openTempFile filename: String) -> Bool { refuseDocument() }
    func application(_ sender: NSApplication, open urls: [URL]) { _ = refuseDocument() }
    func application(_ sender: NSApplication, openFiles filenames: [String]) {
        _ = refuseDocument()
        sender.reply(toOpenOrPrint: .failure)
    }
}
