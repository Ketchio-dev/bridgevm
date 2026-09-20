import AppKit
import Foundation
struct A9ImportRunOutcome {
    var evidence: A9ImportEvidence
    var failureCode: String
    var failureDetail: String
    var cleanupVerified: Bool
    var uiFrontendAutomated: Bool
}

final class A9ImportProductRunner {
    private let request: A9ImportRequest
    private let fileManager: FileManager
    private var application: Process?
    private var ui: T17UIControlling?

    init(request: A9ImportRequest, fileManager: FileManager = .default) {
        self.request = request; self.fileManager = fileManager
    }

    func run() -> A9ImportRunOutcome {
        var evidence = A9ImportEvidence(nonce: request.nonce)
        var failure = "internal-error"; var detail = ""; var automated = false
        do {
            try evidence.prove(.artifactPreflight)
            try authenticateSource(&evidence)
            try evidence.prove(.sourceAuthenticated)
            try prepareLane()
            let process = try launchApplication()
            application = process; _ = T17Activation.bringToFront(pid: process.processIdentifier)
            let control = try T17Accessibility(pid: process.processIdentifier)
            ui = control; automated = true
            try importVM(control)
            try verifyImportedConfiguration()
            try evidence.prove(.uiImported)
            try authenticateImported(&evidence)
            try requireMatchingInitialMedia(evidence)
            try evidence.prove(.importedMediaAuthenticated)
            let ready = try bootToFirstReady(control)
            try evidence.prove(.firstReady)
            try T17GuestJourney(request: request, ui: control, fileManager: fileManager)
                .run(firstReady: ready) { try evidence.prove(A9ImportStage(guest: $0)) }
            try authenticateFinal(&evidence)
            try requireUnchangedSource(evidence)
            failure = "none"
        } catch let blocker as T17Blocker {
            failure = blocker.code; detail = blocker.detail
        } catch {
            failure = "internal-error"; detail = String(describing: error)
        }
        let clean = stopOwnedApplication()
        return A9ImportRunOutcome(evidence: evidence, failureCode: failure, failureDetail: detail,
            cleanupVerified: clean, uiFrontendAutomated: automated)
    }

    private func authenticateSource(_ evidence: inout A9ImportEvidence) throws {
        try evidence.authenticate("source_disk_sha256", file: URL(fileURLWithPath: request.sourceDiskPath))
        try evidence.authenticate("source_vars_sha256", file: URL(fileURLWithPath: request.sourceVarsPath))
        try evidence.authenticateTree("source_vtpm_tree_sha256", root: URL(fileURLWithPath: request.sourceVtpmPath), fileManager: fileManager)
    }

    private func authenticateImported(_ evidence: inout A9ImportEvidence) throws {
        try evidence.authenticate("imported_initial_disk_sha256", file: URL(fileURLWithPath: request.diskPath))
        try evidence.authenticate("imported_initial_vars_sha256", file: URL(fileURLWithPath: request.varsPath))
        try evidence.authenticateTree("imported_initial_vtpm_tree_sha256", root: URL(fileURLWithPath: request.vtpmStatePath), fileManager: fileManager)
    }

    private func authenticateFinal(_ evidence: inout A9ImportEvidence) throws {
        try evidence.authenticate("final_disk_sha256", file: URL(fileURLWithPath: request.diskPath))
        try evidence.authenticate("final_vars_sha256", file: URL(fileURLWithPath: request.varsPath))
        try evidence.authenticateTree("final_vtpm_tree_sha256", root: URL(fileURLWithPath: request.vtpmStatePath), fileManager: fileManager)
        try evidence.authenticate("guest_evidence_sha256", file: URL(fileURLWithPath: request.guestEvidencePath))
    }

    private func requireMatchingInitialMedia(_ evidence: A9ImportEvidence) throws {
        guard evidence.hashes["source_disk_sha256"] == evidence.hashes["imported_initial_disk_sha256"],
              evidence.hashes["source_vars_sha256"] == evidence.hashes["imported_initial_vars_sha256"],
              evidence.hashes["source_vtpm_tree_sha256"] == evidence.hashes["imported_initial_vtpm_tree_sha256"] else {
            throw T17Blocker(code: "import-media-mismatch", detail: "UI-imported media differs from the sealed source")
        }
    }

    private func requireUnchangedSource(_ evidence: A9ImportEvidence) throws {
        guard evidence.hashes["source_disk_sha256"] == (try T17Evidence.sha256(URL(fileURLWithPath: request.sourceDiskPath))),
              evidence.hashes["source_vars_sha256"] == (try T17Evidence.sha256(URL(fileURLWithPath: request.sourceVarsPath))),
              evidence.hashes["source_vtpm_tree_sha256"] == (try A9ImportTreeDigest.compute(URL(fileURLWithPath: request.sourceVtpmPath), fileManager: fileManager)) else {
            throw T17Blocker(code: "source-media-mutated", detail: "the installed-disk import changed its read-only source")
        }
    }

    private func prepareLane() throws {
        try fileManager.createDirectory(atPath: request.libraryRootPath, withIntermediateDirectories: false)
        try fileManager.createDirectory(atPath: request.sharePath, withIntermediateDirectories: false)
        let root = URL(fileURLWithPath: request.sharePath)
        try Data("bridgevm-t17-share-v1\n\(request.nonce)\n".utf8)
            .write(to: root.appendingPathComponent("t17-\(prefix).txt"), options: [.withoutOverwriting])
        try Data("브리지VM T17 클립보드 왕복 v1\n\(request.nonce)\n".utf8)
            .write(to: root.appendingPathComponent("t17-clipboard-host-\(prefix).txt"), options: [.withoutOverwriting])
        for name in ["bv-product-e2e-launch.ps1", "bv-product-e2e.ps1"] {
            let source = URL(fileURLWithPath: request.appBundlePath)
                .appendingPathComponent("Contents/Resources/scripts/win-assets/\(name)")
            guard regularFile(source), (try source.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) < 8 * 1024 * 1024 else {
                throw T17Blocker(code: "app-launch-failed", detail: "packaged product E2E guest asset is missing or oversized")
            }
            try fileManager.copyItem(at: source, to: root.appendingPathComponent(name))
        }
    }

    private func launchApplication() throws -> Process {
        let log = URL(fileURLWithPath: request.laneRoot).appendingPathComponent("import-product-app.log")
        guard fileManager.createFile(atPath: log.path, contents: nil),
              let handle = FileHandle(forWritingAtPath: log.path) else {
            throw T17Blocker(code: "app-launch-failed", detail: "private app log could not be created")
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: request.appExecutablePath)
        process.arguments = ["--e2e-library-root", request.libraryRootPath]
        process.standardOutput = handle; process.standardError = handle
        do { try process.run() } catch {
            try? handle.close()
            throw T17Blocker(code: "app-launch-failed", detail: "packaged app did not launch")
        }
        return process
    }

    private func importVM(_ ui: T17UIControlling) throws {
        try ui.press("bridgevm.first-run.import", timeout: 30)
        try ui.setText(request.vmName, identifier: "bridgevm.first-run.name", timeout: 10)
        try ui.choose(path: request.sourceDiskPath, from: "bridgevm.first-run.disk.choose", timeout: 20)
        try ui.choose(path: request.sourceVarsPath, from: "bridgevm.first-run.vars.choose", timeout: 20)
        try ui.choose(path: request.sourceVtpmPath, from: "bridgevm.first-run.vtpm.choose", timeout: 20)
        try ui.choose(path: request.sourceVtpmPackagePath, from: "bridgevm.first-run.vtpm-package.choose", timeout: 20); try ui.choose(path: request.sourceVtpmCodePath, from: "bridgevm.first-run.vtpm-code.choose", timeout: 20)
        try ui.press("bridgevm.first-run.import.commit", timeout: 15)
        try ui.waitFor("bridgevm.windows.runtime.view", timeout: 1_200)
    }

    private func verifyImportedConfiguration() throws {
        let config = URL(fileURLWithPath: request.libraryRootPath)
            .appendingPathComponent(request.vmSlug).appendingPathComponent("vm.json")
        guard regularFile(config),
              let object = try JSONSerialization.jsonObject(with: Data(contentsOf: config)) as? [String: Any],
              object["id"] as? String == request.vmSlug,
              object["name"] as? String == request.vmName,
              object["backendKind"] as? String == "hvf-engine",
              object["bundlePath"] as? String == request.bundlePath,
              object["diskPath"] as? String == request.diskPath,
              object["installPending"] as? Bool != true else {
            throw T17Blocker(code: "import-registration-invalid", detail: "persisted imported VM identity is incomplete or mismatched")
        }
    }

    private func bootToFirstReady(_ ui: T17UIControlling) throws -> String {
        try ui.setToggle(true, identifier: "bridgevm.runtime.clipboard", timeout: 10)
        try ui.setToggle(true, identifier: "bridgevm.runtime.network", timeout: 10)
        try ui.setToggle(true, identifier: "bridgevm.runtime.share.enabled", timeout: 10)
        try ui.setText(request.sharePath, identifier: "bridgevm.runtime.share.host", timeout: 10)
        try ui.setText("C:\\bridgevm-share", identifier: "bridgevm.runtime.share.guest", timeout: 10)
        try ui.press("bridgevm.windows.runtime.start", timeout: 20)
        return try T17FirstReadyWaiter.wait(observe: {
            let ready = self.boundedLines(self.runLog).first { $0.hasPrefix("BVAGENT READY") }
            return try T17FirstReadyObservation.capture(
                readyLine: ready, applicationRunning: self.application?.isRunning == true, ui: ui)
        }, diagnostic: { T17FirstBootDiagnostic.capture(self.runLog) })
    }

    private func stopOwnedApplication() -> Bool {
        if let ui { try? ui.press("bridgevm.windows.runtime.stop", timeout: 2) }
        _ = wait(timeout: 30) { self.boundedLines(self.runLog).contains { $0.contains("stop: PSCI SYSTEM_OFF") } }
        guard let application else { return true }
        if application.isRunning { application.terminate() }
        _ = wait(timeout: 10) { !application.isRunning }
        if application.isRunning { application.interrupt() }
        _ = wait(timeout: 5) { !application.isRunning }
        return !application.isRunning && A9ImportedKeyCleanup.run(request: request, fileManager: fileManager)
    }

    private func regularFile(_ url: URL) -> Bool {
        guard let v = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]) else { return false }
        return v.isRegularFile == true && v.isSymbolicLink != true && (v.fileSize ?? 0) > 0
    }

    private func boundedLines(_ url: URL) -> [String] {
        guard regularFile(url), let handle = FileHandle(forReadingAtPath: url.path) else { return [] }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        try? handle.seek(toOffset: size > 8 * 1024 * 1024 ? size - 8 * 1024 * 1024 : 0)
        return String(decoding: (try? handle.readToEnd()) ?? Data(), as: UTF8.self)
            .split(whereSeparator: \.isNewline).map(String.init)
    }

    private func wait(timeout: TimeInterval, predicate: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if predicate() { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        return predicate()
    }

    private var prefix: String { String(request.nonce.prefix(12)) }
    private var runLog: URL { URL(fileURLWithPath: request.bundlePath).appendingPathComponent("logs/hvf/run.log") }
}
