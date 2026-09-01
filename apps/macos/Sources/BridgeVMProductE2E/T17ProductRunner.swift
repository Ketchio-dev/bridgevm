import AppKit
import CryptoKit
import Foundation

struct T17RunOutcome {
    var evidence: T17Evidence
    var failureCode: String
    var cleanupVerified: Bool
    var installerSourcePath: String
    var uiFrontendAutomated: Bool
}

final class T17ProductRunner {
    private let request: T17Request
    private let fileManager: FileManager
    private var application: Process?
    private var ui: T17UIControlling?

    init(request: T17Request, fileManager: FileManager = .default) {
        self.request = request
        self.fileManager = fileManager
    }

    func run() -> T17RunOutcome {
        var evidence = T17Evidence(nonce: request.nonce)
        var failure = "internal-error"
        var sourcePath = "absent"
        var uiFrontendAutomated = false
        do {
            try evidence.prove(.artifactPreflight)
            try prepareLane()
            let process = try launchApplication()
            application = process
            let control = try T17Accessibility(pid: process.processIdentifier)
            ui = control
            uiFrontendAutomated = true
            try createVM(control)
            try verifyCreatedVM()
            try evidence.prove(.vmCreated)
            try installWindows(control)
            let source = try productInstallerSource()
            sourcePath = source.path
            try evidence.authenticate("installer_source_sha256", file: source)
            try evidence.prove(.sourcePrepared)
            try verifyInstalledVM()
            try evidence.authenticate("final_disk_sha256", file: URL(fileURLWithPath: request.diskPath))
            try evidence.authenticate("final_vars_sha256", file: URL(fileURLWithPath: request.varsPath))
            try evidence.prove(.windowsInstalled)
            try verifySecureBootReceipt()
            try evidence.authenticate("secure_boot_receipt_sha256", file: URL(fileURLWithPath: request.secureBootReceiptPath))
            try evidence.prove(.secureBootProvisioned)
            let readyLine = try bootToFirstReady(control)
            try evidence.prove(.firstReady)
            evidence = try T17GuestJourney(request: request, ui: control, fileManager: fileManager)
                .run(evidence: evidence, firstReady: readyLine)
            try evidence.authenticate("final_disk_sha256", file: URL(fileURLWithPath: request.diskPath))
            try evidence.authenticate("final_vars_sha256", file: URL(fileURLWithPath: request.varsPath))
            try evidence.authenticate("guest_evidence_sha256", file: URL(fileURLWithPath: request.guestEvidencePath))
            failure = "none"
        } catch let blocker as T17Blocker {
            failure = blocker.code
        } catch {
            failure = "internal-error"
        }
        let clean = stopOwnedApplication()
        return T17RunOutcome(evidence: evidence, failureCode: failure,
                             cleanupVerified: clean, installerSourcePath: sourcePath,
                             uiFrontendAutomated: uiFrontendAutomated)
    }

    private func prepareLane() throws {
        try fileManager.createDirectory(atPath: request.libraryRootPath, withIntermediateDirectories: false)
        try fileManager.createDirectory(atPath: request.sharePath, withIntermediateDirectories: false)
        try T17PrivateUnattend.write(to: privateUnattend, nonce: request.nonce, fileManager: fileManager)
        let shareProbe = URL(fileURLWithPath: request.sharePath).appendingPathComponent("t17-\(request.nonce.prefix(12)).txt")
        try Data("bridgevm-t17-share-v1\n\(request.nonce)\n".utf8).write(to: shareProbe, options: [.withoutOverwriting])
        let clipboardProbe = URL(fileURLWithPath: request.sharePath)
            .appendingPathComponent("t17-clipboard-host-\(request.nonce.prefix(12)).txt")
        try Data("브리지VM T17 클립보드 왕복 v1\n\(request.nonce)\n".utf8)
            .write(to: clipboardProbe, options: [.withoutOverwriting])
        for name in ["bv-product-e2e-launch.ps1", "bv-product-e2e.ps1"] {
            let asset = URL(fileURLWithPath: request.appBundlePath)
                .appendingPathComponent("Contents/Resources/scripts/win-assets/\(name)")
            guard regularFile(asset), (try asset.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) < 8 * 1024 * 1024 else {
                throw T17Blocker(code: "app-launch-failed", detail: "packaged product E2E guest asset is missing or oversized")
            }
            try fileManager.copyItem(at: asset, to: URL(fileURLWithPath: request.sharePath).appendingPathComponent(name))
        }
    }

    private func launchApplication() throws -> Process {
        let log = URL(fileURLWithPath: request.laneRoot).appendingPathComponent("product-app.log")
        guard fileManager.createFile(atPath: log.path, contents: nil),
              let handle = FileHandle(forWritingAtPath: log.path) else {
            throw T17Blocker(code: "app-launch-failed", detail: "private app log could not be created")
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: request.appExecutablePath)
        process.arguments = ["--e2e-library-root", request.libraryRootPath,
                             "--e2e-unattend-path", privateUnattend.path]
        process.standardOutput = handle; process.standardError = handle
        do { try process.run() } catch {
            try? handle.close()
            throw T17Blocker(code: "app-launch-failed", detail: "packaged app did not launch")
        }
        return process
    }

    private func createVM(_ ui: T17UIControlling) throws {
        try ui.press("bridgevm.library.empty.create", timeout: 30)
        try ui.press("bridgevm.create.os.windows", timeout: 10)
        try ui.press("bridgevm.create.windows.install", timeout: 10)
        try ui.setText(request.vmName, identifier: "bridgevm.create.name", timeout: 10)
        try ui.choose(path: request.isoPath, from: "bridgevm.create.windows.iso", timeout: 15)
        try ui.choose(path: request.guestPayloadPath, from: "bridgevm.create.windows.guest-payload", timeout: 15)
        try ui.choose(path: request.guestPayloadManifestPath, from: "bridgevm.create.windows.guest-manifest", timeout: 15)
        try ui.press("bridgevm.create.commit", timeout: 15)
        try ui.waitFor("bridgevm.windows.install.view", timeout: 60)
    }

    private func installWindows(_ ui: T17UIControlling) throws {
        try ui.press("bridgevm.install.start", timeout: 20)
        let completed = waitUntil(timeout: 1_800) {
            guard self.application?.isRunning == true else { return false }
            return (try? ui.text("bridgevm.windows.install.stage", timeout: 1)) == "완료"
        }
        guard completed else {
            throw T17Blocker(code: "installer-failed", detail: "product install did not reach the completed UI stage")
        }
    }

    private func verifyCreatedVM() throws {
        let config = vmRoot.appendingPathComponent("vm.json")
        let object = try jsonObject(config)
        guard object["name"] as? String == request.vmName,
              object["id"] as? String == request.vmSlug,
              object["installPending"] as? Bool == true,
              object["bundlePath"] as? String == bundle.path else {
            throw T17Blocker(code: "vm-creation-failed", detail: "UI-created VM config does not match the sealed identity")
        }
        let managedISO = bundle.appendingPathComponent("disks/installer.iso")
        let managedPayload = bundle.appendingPathComponent("metadata/windows-guest-payload", isDirectory: true)
        let managedManifest = bundle.appendingPathComponent("metadata/windows-guest-payload.tsv")
        let managedUnattend = bundle.appendingPathComponent("metadata/windows-install-unattend.xml")
        guard try T17Evidence.sha256(managedISO) == T17Evidence.sha256(URL(fileURLWithPath: request.isoPath)),
              try T17Evidence.sha256(managedManifest) == T17Evidence.sha256(URL(fileURLWithPath: request.guestPayloadManifestPath)),
              try T17Evidence.sha256(managedUnattend) == T17Evidence.sha256(privateUnattend),
              try treeDigest(managedPayload) == treeDigest(URL(fileURLWithPath: request.guestPayloadPath)) else {
            throw T17Blocker(code: "vm-creation-failed", detail: "managed install inputs differ from the UI-selected inputs")
        }
    }

    private func verifyInstalledVM() throws {
        let object = try jsonObject(vmRoot.appendingPathComponent("vm.json"))
        guard object["installPending"] as? Bool == false,
              regularFile(URL(fileURLWithPath: request.diskPath)),
              regularFile(URL(fileURLWithPath: request.varsPath)),
              fileManager.fileExists(atPath: bundle.appendingPathComponent("metadata/hvf-install-done.json").path) else {
            throw T17Blocker(code: "installer-failed", detail: "completed UI stage lacks durable installed media")
        }
    }

    private func verifySecureBootReceipt() throws {
        try T17SecureBootReceipt.verify(
            receipt: URL(fileURLWithPath: request.secureBootReceiptPath),
            policy: URL(fileURLWithPath: request.secureBootPolicyPath))
    }

    private func productInstallerSource() throws -> URL {
        let directory = URL(fileURLWithPath: request.libraryRootPath)
            .appendingPathComponent("Derived/WindowsInstallSources", isDirectory: true)
        let candidates = (try? fileManager.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey]
        ))?.filter { regularFile($0) && $0.lastPathComponent.range(
            of: #"^win11-[0-9a-f]{64}\.raw$"#, options: .regularExpression) != nil } ?? []
        guard candidates.count == 1 else {
            throw T17Blocker(code: "installer-failed", detail: "product source cache identity is missing or ambiguous")
        }
        let source = candidates[0]
        let receipt = URL(fileURLWithPath: source.path + ".sha256")
        guard regularFile(receipt),
              (try String(contentsOf: receipt, encoding: .utf8)) == (try T17Evidence.sha256(source)) + "\n" else {
            throw T17Blocker(code: "installer-failed", detail: "product source cache digest receipt is missing or stale")
        }
        return source
    }

    private func bootToFirstReady(_ ui: T17UIControlling) throws -> String {
        try ui.waitFor("bridgevm.windows.runtime.view", timeout: 60)
        try ui.setToggle(true, identifier: "bridgevm.runtime.clipboard", timeout: 10)
        try ui.setToggle(true, identifier: "bridgevm.runtime.network", timeout: 10)
        try ui.setToggle(true, identifier: "bridgevm.runtime.share.enabled", timeout: 10)
        try ui.setText(request.sharePath, identifier: "bridgevm.runtime.share.host", timeout: 10)
        try ui.setText("C:\\bridgevm-share", identifier: "bridgevm.runtime.share.guest", timeout: 10)
        try ui.press("bridgevm.windows.runtime.start", timeout: 20)
        let log = bundle.appendingPathComponent("logs/hvf/run.log")
        var ready: String?
        let observed = waitUntil(timeout: 600) {
            ready = self.boundedLines(log).first { $0.hasPrefix("BVAGENT READY") }
            return ready != nil
        }
        guard observed, let ready else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "first boot has no BVAGENT READY evidence")
        }
        return ready
    }

    private func stopOwnedApplication() -> Bool {
        if let ui { try? ui.press("bridgevm.windows.runtime.stop", timeout: 2) }
        _ = waitUntil(timeout: 30) { self.boundedLines(self.bundle.appendingPathComponent("logs/hvf/run.log"))
            .contains(where: { $0.contains("stop: PSCI SYSTEM_OFF") }) }
        guard let application else { return true }
        if application.isRunning { application.terminate() }
        _ = waitUntil(timeout: 10) { !application.isRunning }
        if application.isRunning { application.interrupt() }
        _ = waitUntil(timeout: 5) { !application.isRunning }
        return !application.isRunning
    }

    private var vmRoot: URL { URL(fileURLWithPath: request.libraryRootPath).appendingPathComponent(request.vmSlug) }
    private var bundle: URL { vmRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true) }
    private var privateUnattend: URL { URL(fileURLWithPath: request.laneRoot).appendingPathComponent("e2e-unattend.xml") }

    private func waitUntil(timeout: TimeInterval, _ predicate: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if predicate() { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        return predicate()
    }

    private func regularFile(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]) else { return false }
        return values.isRegularFile == true && values.isSymbolicLink != true && (values.fileSize ?? 0) > 0
    }

    private func jsonObject(_ url: URL) throws -> [String: Any] {
        guard regularFile(url), let value = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any] else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "required product JSON is missing or malformed")
        }
        return value
    }

    private func boundedLines(_ url: URL) -> [String] {
        guard regularFile(url), let handle = FileHandle(forReadingAtPath: url.path) else { return [] }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        try? handle.seek(toOffset: size > 8 * 1024 * 1024 ? size - 8 * 1024 * 1024 : 0)
        let data = (try? handle.readToEnd()) ?? Data()
        return String(decoding: data, as: UTF8.self).split(whereSeparator: \.isNewline).map(String.init)
    }

    private func treeDigest(_ root: URL) throws -> String {
        let values = try root.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true,
              let enumerator = fileManager.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey]) else {
            throw T17Blocker(code: "vm-creation-failed", detail: "managed guest payload is missing")
        }
        var records: [String] = []
        for case let item as URL in enumerator {
            let info = try item.resourceValues(forKeys: [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey])
            guard info.isSymbolicLink != true else { throw T17Blocker(code: "vm-creation-failed", detail: "guest payload contains a symlink") }
            let relative = String(item.path.dropFirst(root.path.count + 1))
            if info.isDirectory == true { records.append("D\t\(relative)\n") }
            else if info.isRegularFile == true { records.append("F\t\(relative)\t\(try T17Evidence.sha256(item))\n") }
            else { throw T17Blocker(code: "vm-creation-failed", detail: "guest payload contains a non-regular entry") }
        }
        return SHA256.hash(data: Data(records.sorted().joined().utf8)).map { String(format: "%02x", $0) }.joined()
    }
}
