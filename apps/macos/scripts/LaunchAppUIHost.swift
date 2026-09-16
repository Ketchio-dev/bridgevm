import AppKit
import CryptoKit
import Darwin
import Foundation

enum AppUIHostLauncherError: Error { case refused(String) }

struct AppUIHostLaunchPaths {
    let app: URL
    let output: URL
    let receipt: URL
    let sha256: String
    var cancellation: URL { output.deletingLastPathComponent().appendingPathComponent("cancel.requested") }

    init(arguments: [String]) throws {
        guard arguments.count == 8, arguments[0] == "--app", arguments[2] == "--output",
              arguments[4] == "--host-sha256", arguments[6] == "--receipt" else {
            throw AppUIHostLauncherError.refused("Expected --app APP --output DIRECTORY --host-sha256 SHA256 --receipt FILE")
        }
        func canonical(_ path: String) throws -> URL {
            let url = URL(fileURLWithPath: path).standardizedFileURL
            guard path.hasPrefix("/"), path.utf8.count <= 4096, !path.contains("\0"),
                  !(path as NSString).pathComponents.contains(".."),
                  !(path as NSString).pathComponents.contains("."), url.path == path,
                  url.resolvingSymlinksInPath().path == path else {
                throw AppUIHostLauncherError.refused("Launcher paths must be canonical and contain no symlinks")
            }
            return url
        }
        app = try canonical(arguments[1])
        output = try canonical(arguments[3])
        receipt = try canonical(arguments[7])
        sha256 = arguments[5]
        let parent = output.deletingLastPathComponent()
        guard parent.lastPathComponent == "app-ui-private", output.lastPathComponent == "host-observations",
              app == parent.appendingPathComponent("BridgeVMAppUIHost.app"),
              receipt == parent.appendingPathComponent("launcher-observations.json"),
              sha256.count == 64, sha256.allSatisfy({ "0123456789abcdef".contains($0) }) else {
            throw AppUIHostLauncherError.refused("Launcher paths or SHA differ from the fixed diagnostic contract")
        }
        for directory in [parent, output] {
            let attrs = try FileManager.default.attributesOfItem(atPath: directory.path)
            guard attrs[.type] as? FileAttributeType == .typeDirectory,
                  (attrs[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid(),
                  (attrs[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
                throw AppUIHostLauncherError.refused("Diagnostic output requires owned 0700 directories")
            }
        }
        guard try FileManager.default.contentsOfDirectory(atPath: output.path).isEmpty,
              !Self.exists(receipt.path), let bundle = Bundle(url: app),
              bundle.bundleIdentifier == "dev.bridgevm.app-ui-host",
              bundle.executableURL?.standardizedFileURL == app.appendingPathComponent("Contents/MacOS/BridgeVMControl"),
              try Self.digest(app.appendingPathComponent("Contents/MacOS/BridgeVMControl")) == sha256 else {
            throw AppUIHostLauncherError.refused("Diagnostic bundle, hash, or fresh output validation failed")
        }
    }
    static func exists(_ path: String) -> Bool {
        var info = stat()
        return lstat(path, &info) == 0 || errno != ENOENT
    }
    static func digest(_ url: URL) throws -> String {
        let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
        guard url.resolvingSymlinksInPath() == url, attrs[.type] as? FileAttributeType == .typeRegular,
              let size = (attrs[.size] as? NSNumber)?.int64Value, size > 0, size <= 536_870_912 else {
            throw AppUIHostLauncherError.refused("Executable is not a bounded regular sealed file")
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var digest = SHA256()
        while let bytes = try handle.read(upToCount: 1_048_576), !bytes.isEmpty { digest.update(data: bytes) }
        return digest.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

@MainActor
final class AppUIHostLauncher {
    let paths: AppUIHostLaunchPaths
    private var ownership: AppUIHostLaunchOwnership
    private var application: NSRunningApplication?
    private var observer: NSObjectProtocol?
    private var signals: [DispatchSourceSignal] = []
    private var lastReceipt: Data?
    private var receiptFailed = false
    private var wroteCancellation = false
    private var now: TimeInterval { ProcessInfo.processInfo.systemUptime }

    init(paths: AppUIHostLaunchPaths, startedAt: TimeInterval) {
        self.paths = paths
        ownership = AppUIHostLaunchOwnership(expectation: AppUIHostLaunchExpectation(
            bundlePath: paths.app.path, executableSHA256: paths.sha256,
            requestedAt: Date().timeIntervalSince1970), startedAt: startedAt)
    }
    func run() -> Int32 {
        save()
        installSignals()
        defer {
            if let observer { NSWorkspace.shared.notificationCenter.removeObserver(observer) }
            for source in signals { source.cancel() }
        }
        // Subscribe before launch: notification ownership is independent of the
        // callback and of any identity file the host has had time to write.
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didLaunchApplicationNotification, object: nil, queue: nil
        ) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
            DispatchQueue.main.async { self?.observe(app, callback: false) }
        }
        if now >= ownership.startedAt + 90 { stop(.deadline) }
        else if AppUIHostLaunchPaths.exists(paths.cancellation.path) { stop(.cancelled) }
        else if !receiptFailed {
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.createsNewApplicationInstance = true
            configuration.allowsRunningApplicationSubstitution = false
            configuration.addsToRecentItems = false
            configuration.promptsUserIfNeeded = false
            configuration.environment = ["BRIDGEVM_APP_UI_HOST_MODE": "1", "BRIDGEVM_APP_UI_HOST_OUTPUT": paths.output.path]
            NSWorkspace.shared.openApplication(at: paths.app, configuration: configuration) { [weak self] app, error in
                DispatchQueue.main.async {
                    guard let self else { return }
                    if let app { self.observe(app, callback: true) }
                    else { self.stop(.failure("NSWorkspace launch failed: \(String(describing: error))")) }
                }
            }
        }
        while !ownership.finished {
            if !wroteCancellation && AppUIHostLaunchPaths.exists(paths.cancellation.path) { stop(.cancelled) }
            let action = ownership.poll(now: now, ownedProcessExited: application?.isTerminated == true)
            if action == .terminate || action == .kill { signalOwnedApplication(action) }
            if ownership.timedOut { markCancellation() }
            save()
            if !ownership.finished { _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        }
        save()
        return ownership.success && !receiptFailed ? 0 : 2
    }
    private func identity(_ app: NSRunningApplication) throws -> AppUIHostProcessIdentity {
        guard !app.isTerminated, let date = app.launchDate, let bundle = app.bundleURL?.standardizedFileURL,
              let executable = app.executableURL?.standardizedFileURL,
              bundle.resolvingSymlinksInPath() == bundle, executable.resolvingSymlinksInPath() == executable,
              bundle == paths.app, executable.path == ownership.expectation.executablePath,
              let identifier = app.bundleIdentifier, identifier == "dev.bridgevm.app-ui-host" else {
            throw AppUIHostLauncherError.refused("Running application has no complete live identity")
        }
        return AppUIHostProcessIdentity(pid: app.processIdentifier, launchDate: date.timeIntervalSince1970,
            bundleIdentifier: identifier, bundlePath: bundle.path, executablePath: executable.path,
            executableSHA256: try AppUIHostLaunchPaths.digest(executable))
    }
    private func observe(_ app: NSRunningApplication, callback: Bool) {
        guard !ownership.finished else { return }
        if !callback && app.bundleURL?.standardizedFileURL != paths.app { return }
        do {
            let observed = try identity(app)
            if ownership.observe(observed, now: now) { application = app }
            else { markCancellation() }
        } catch { stop(.failure("Could not verify launched application: \(error)")) }
        save()
    }
    private func stop(_ reason: AppUIHostLaunchOwnership.StopReason) {
        ownership.requestStop(reason, now: now)
        markCancellation()
        save()
    }
    private func markCancellation() {
        guard !AppUIHostLaunchPaths.exists(paths.cancellation.path) else { return }
        do {
            try Data("cancel\n".utf8).write(to: paths.cancellation, options: .withoutOverwriting)
            wroteCancellation = true
        }
        catch {
            ownership.requestStop(.failure("Cancellation marker could not be written"), now: now)
        }
    }
    private func signalOwnedApplication(_ action: AppUIHostLaunchOwnership.Action) {
        if application?.isTerminated == true {
            _ = ownership.poll(now: now, ownedProcessExited: true)
            return
        }
        let current: AppUIHostProcessIdentity?
        if let pid = ownership.identity?.pid, let live = NSRunningApplication(processIdentifier: pid) {
            current = try? identity(live)
        } else { current = nil }
        if current == nil && application?.isTerminated == true {
            _ = ownership.poll(now: now, ownedProcessExited: true)
            return
        }
        guard let pid = ownership.authorize(action, current: current, now: now) else { return }
        // NSRunningApplication.terminate is a normal quit request, not SIGTERM.
        if Darwin.kill(pid, action == .terminate ? SIGTERM : SIGKILL) != 0 && errno != ESRCH {
            ownership.requestStop(.failure("Verified application termination signal failed"), now: now)
        }
    }
    private func installSignals() {
        for number in [SIGTERM, SIGINT] {
            Darwin.signal(number, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: number, queue: .main)
            source.setEventHandler { [weak self] in
                DispatchQueue.main.async { self?.stop(.cancelled) }
            }
            source.resume()
            signals.append(source)
        }
    }
    private func save() {
        do {
            let data = try JSONSerialization.data(withJSONObject: ownership.receipt, options: [.prettyPrinted, .sortedKeys])
            guard data != lastReceipt else { return }
            try data.write(to: paths.receipt, options: lastReceipt == nil ? .withoutOverwriting : .atomic)
            lastReceipt = data
        } catch {
            receiptFailed = true
            ownership.requestStop(.failure("Launcher receipt could not be saved"), now: now)
            markCancellation()
            FileHandle.standardError.write(Data("Launcher receipt could not be saved.\n".utf8))
        }
    }
}

@main
enum LaunchAppUIHost {
    @MainActor static func main() {
        umask(0o077)
        let startedAt = ProcessInfo.processInfo.systemUptime
        do {
            let paths = try AppUIHostLaunchPaths(arguments: Array(CommandLine.arguments.dropFirst()))
            exit(AppUIHostLauncher(paths: paths, startedAt: startedAt).run())
        } catch {
            FileHandle.standardError.write(Data("Native app launcher refused: \(error)\n".utf8))
            exit(2)
        }
    }
}
