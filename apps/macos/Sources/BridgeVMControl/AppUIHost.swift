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
    private weak var window: NSWindow?
    private var launchFinished = false
    private var started = false
    private var finished = false
    private var monitor: Task<Void, Never>?
    private var scenario: Task<Void, Never>?

    static func outputDirectory(arguments: [String], fileManager: FileManager = .default) throws -> URL {
        guard arguments.count == 3, arguments[0] == "--app-ui-host", arguments[1] == "--output" else {
            throw AppUIHostError.refused("Expected exactly --app-ui-host --output ABSOLUTE_DIRECTORY")
        }
        let path = arguments[2]
        let components = (path as NSString).pathComponents
        let output = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
        guard path.hasPrefix("/"), path.utf8.count <= 4096, !path.contains("\0"),
              !components.contains(".."), !components.contains("."), output.path == path,
              output.resolvingSymlinksInPath().path == path, output.lastPathComponent == "host-observations",
              output.deletingLastPathComponent().lastPathComponent == "app-ui-private" else {
            throw AppUIHostError.refused("Output must be a canonical app-ui-private/host-observations directory")
        }
        for directory in [output.deletingLastPathComponent(), output] {
            let attributes = try fileManager.attributesOfItem(atPath: directory.path)
            guard attributes[.type] as? FileAttributeType == .typeDirectory,
                  (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid(),
                  (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
                throw AppUIHostError.refused("Output and parent require owned 0700 directories")
            }
        }
        guard try fileManager.contentsOfDirectory(atPath: path).isEmpty else {
            throw AppUIHostError.refused("Output directory must be empty")
        }
        return output
    }
    static func prepare(arguments: [String]) throws {
        guard prepared == nil else { throw AppUIHostError.refused("Host already prepared") }
        let output = try outputDirectory(arguments: arguments)
        let capture = try AppUIHostCapture(output: output)
        do {
            try writeIdentity(capture)
            try checkCancellation(output: output)
            let host = try AppUIHost(capture: capture)
            prepared = host
            host.armMonitor()
        } catch {
            capture.failed(error)
            try? capture.writeCompletion(cleanupVerified: false)
            throw error
        }
    }
    private static func writeIdentity(_ capture: AppUIHostCapture) throws {
        let bundle = Bundle.main.bundleURL.standardizedFileURL
        guard Bundle.main.bundleIdentifier == "dev.bridgevm.app-ui-host",
              bundle.lastPathComponent == "BridgeVMAppUIHost.app", bundle.resolvingSymlinksInPath() == bundle,
              let executable = Bundle.main.executableURL?.standardizedFileURL,
              executable == bundle.appendingPathComponent("Contents/MacOS/BridgeVMControl"),
              executable.resolvingSymlinksInPath() == executable,
              (try executable.resourceValues(forKeys: [.isRegularFileKey])).isRegularFile == true else {
            throw AppUIHostError.refused("Host bundle/executable identity differs from the fixed diagnostic contract")
        }
        try capture.write(["schema_version": 1, "kind": "native-app-ui-host-identity", "pid": Int(getpid()),
            "bundle_identifier": "dev.bridgevm.app-ui-host", "bundle_path": bundle.path,
            "executable_path": executable.path, "executable_sha256": try AppUIHostCapture.digest(executable),
            "started_uptime": ProcessInfo.processInfo.systemUptime], name: "host-identity.json", final: true)
    }
    private init(capture: AppUIHostCapture) throws {
        self.capture = capture
        root = capture.output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false,
                                               attributes: [.posixPermissions: 0o700])
        library = Self.makeLibrary(root: root, capture: capture)
    }
    static func makeLibrary(root: URL, capture: AppUIHostCapture) -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false,
            installSessionFactory: { _ in capture.tripwire("install_creations") },
            runtimeSessionFactory: { _ in capture.tripwire("runtime_creations") },
            actionScheduler: { _ in capture.tripwire("file_jobs") }, startsModelsAutomatically: false,
            modelFactory: { _ in capture.tripwire("model_creations") })
    }
    static func checkCancellation(output: URL) throws {
        let marker = output.deletingLastPathComponent().appendingPathComponent("cancel.requested").path
        var status = stat()
        if lstat(marker, &status) == 0 { throw AppUIHostError.refused("Host canceled by owning launcher") }
        guard errno == ENOENT else { throw AppUIHostError.refused("Cancellation marker could not be inspected") }
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
        monitor = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, !self.finished else { return }
                do {
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
    func applicationDidFinishLaunching() {
        guard !finished else { return }
        launchFinished = true
        startWhenReady()
    }
    func attach(window: NSWindow) {
        guard !finished else { return }
        guard self.window == nil || self.window === window else {
            finish(error: AppUIHostError.refused("A second app content window appeared")); return
        }
        self.window = window
        startWhenReady()
    }
    private func startWhenReady() {
        guard launchFinished, !started, !finished, let window, window.isVisible,
              let content = window.contentView else { return }
        started = true
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
    private func finish(error: Error?, terminateApplication: Bool = true) {
        guard !finished else { return }
        finished = true
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
