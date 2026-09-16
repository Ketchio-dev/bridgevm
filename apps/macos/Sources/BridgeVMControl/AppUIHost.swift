#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Foundation
import Darwin

@MainActor
final class AppUIHost {
    static private(set) var prepared: AppUIHost?
    let root: URL
    let library: LibraryModel
    let capture: AppUIHostCapture
    let lifecycle: AppUIHostLifecycle
    private weak var window: NSWindow?
    private var launchFinished = false
    private var started = false
    private var finished = false
    private var monitor: Task<Void, Never>?
    private var scenario: Task<Void, Never>?

    static func prepare(arguments: [String], environment: [String: String] = [:]) throws {
        guard prepared == nil else { throw AppUIHostError.refused("Host already prepared") }
        let before = AppUIHostPreparationObservation.snapshot()
        let request = try AppUIHostRequest.resolve(arguments: arguments, environment: environment)
        let output = request.output
        let capture = try AppUIHostCapture(output: output)
        do {
            try writeIdentity(capture)
            try checkCancellation(output: output)
            let host = try AppUIHost(capture: capture)
            prepared = host
            host.armMonitor()
            try AppUIHostPreparationObservation.write(capture, request: request,
                environment: environment, before: before)
        } catch {
            if let host = prepared { host.finish(error: error, terminateApplication: false) }
            else {
                capture.failed(error)
                try? capture.writeCompletion(cleanupVerified: false)
            }
            throw error
        }
    }
    private init(capture: AppUIHostCapture) throws {
        self.capture = capture
        lifecycle = try AppUIHostLifecycle(capture: capture)
        root = capture.output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false,
                                               attributes: [.posixPermissions: 0o700])
        library = Self.makeLibrary(root: root, capture: capture)
    }
    func checkBeforeApplication() throws {
        do { try Self.checkCancellation(output: capture.output) }
        catch { finish(error: error, terminateApplication: false); throw error }
    }
    func libraryForApplication() -> LibraryModel {
        do { try checkBeforeApplication() } catch { exit(2) }
        return library
    }
    private func armMonitor() {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        lifecycle.setStartupDeadline(deadline)
        monitor = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, !self.finished else { return }
                do {
                    self.observeLifecycle()
                    try Self.checkCancellation(output: self.capture.output)
                    if !self.started && ProcessInfo.processInfo.systemUptime >= deadline {
                        throw AppUIHostError.refused("Timed out waiting for actual app launch and window")
                    }
                    self.startWhenReady()
                    try await Task.sleep(nanoseconds: 50_000_000)
                } catch is CancellationError { return }
                catch { self.finish(error: error); return }
            }
        }
    }
    func applicationDidFinishLaunching(_ notification: Notification) {
        lifecycle.recordDelegate(notification)
        guard !finished else { return }
        launchFinished = true
        startWhenReady()
    }
    func attachmentChanged(window: NSWindow?) {
        lifecycle.record(window == nil ? .attachmentNil : .attachmentNonnull)
        if let window { attach(window: window) }
    }
    private func attach(window: NSWindow) {
        guard !finished else { return }
        guard self.window == nil || self.window === window else {
            finish(error: AppUIHostError.refused("A second app content window appeared")); return
        }
        self.window = window
        startWhenReady()
    }
    private func startWhenReady() {
        observeLifecycle()
        guard launchFinished, !started, !finished, let window, window.isVisible,
              let content = window.contentView else { return }
        started = true
        observeLifecycle()
        scenario = Task { [weak self] in
            guard let self else { return }
            do {
                try Self.checkCancellation(output: self.capture.output)
                try self.capture.bindOwner(window)
                try await AppUIHostScenario.run(self, window: window, content: content)
                self.finish(error: nil)
            } catch { self.finish(error: error) }
        }
    }
    func seedOverviewMetadata() {
        library.vms = [("Amber", 4096), ("Indigo", 8192)].map { name, memory in
            VMConfig(id: "ui-" + name.lowercased(), name: name, displayName: name,
                backendKind: "hvf-engine", bootMode: "windows-hvf",
                bundlePath: root.appendingPathComponent(name + ".vmbridge").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: "owned-ui-fixture", displayWidth: 1280, displayHeight: 800,
                memMiB: memory, cpuCount: 4)
        }
    }
    private func observeLifecycle(terminal: Bool = false) {
        lifecycle.observe(launchFinished: launchFinished, started: started, finished: finished,
                          window: window, terminal: terminal)
    }
    private func finish(error: Error?, terminateApplication: Bool = true) {
        guard !finished else { return }
        finished = true
        observeLifecycle(terminal: true)
        monitor?.cancel()
        scenario?.cancel()
        if let error { capture.failed(error) }
        var clean = false
        do {
            guard try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty else {
                throw AppUIHostError.refused("UI-only fixture unexpectedly wrote to its library")
            }
            try FileManager.default.removeItem(at: root)
            clean = true
        } catch { capture.failed(error) }
        do { try capture.writeCompletion(cleanupVerified: clean) }
        catch {
            capture.failed(error)
            FileHandle.standardError.write(Data("App UI host completion could not be saved.\n".utf8))
        }
        if terminateApplication {
            guard let application = NSApp else { exit(2) }
            application.terminate(nil)
        }
    }
}
#endif
