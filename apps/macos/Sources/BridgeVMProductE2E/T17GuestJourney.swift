import Foundation

struct T17GuestJourney {
    let request: T17Request
    let ui: T17UIControlling
    let fileManager: FileManager

    func run(evidence initial: T17Evidence, firstReady: String) throws -> T17Evidence {
        guard currentLogText().contains(firstReady) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "first READY record changed before the product journey")
        }
        var evidence = initial
        try keyboardAndPointer(); try evidence.prove(.keyboardPointer)
        try clipboard(); try evidence.prove(.clipboard)
        try workload("Share", output: "t17-guest-\(prefix).txt"); try evidence.prove(.folderShare)
        try workload("Network", output: "t17-network-\(prefix).txt"); try evidence.prove(.network)
        try workload("Audio", output: "t17-audio-\(prefix).txt")
        try workload("MarkerA", output: "t17-snapshot-a-\(prefix).txt")
        try shutdown()
        guard T17RunLogProof.audioPassed(runLog) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest audio lacked successful host CoreAudio counters")
        }
        try evidence.prove(.audio); try evidence.prove(.firstShutdown)
        let first = try archiveLog(named: "first-run.log", ready: "first-ready", shutdown: "first-shutdown")
        try createSnapshot()
        _ = try bootReady()
        try workload("MarkerB", output: "t17-snapshot-b-\(prefix).txt")
        try shutdown()
        let mutation = try archiveLog(named: "mutation-run.log", ready: "mutation-ready", shutdown: "mutation-shutdown")
        try restoreSnapshot()
        _ = try bootReady()
        try workload("MarkerRestoredA", output: "t17-snapshot-restored-a-\(prefix).txt")
        try workload("AgentResult", output: "t17-agent-result-\(prefix).json", includeIdentity: true)
        let artifacts = try T17GuestArtifacts.collect(request: request, fileManager: fileManager)
        try evidence.prove(.snapshotRestore); try evidence.prove(.secondReady)
        try shutdown(); try evidence.prove(.secondShutdown)
        let final = try T17RunLogProof.capture(runLog, nonce: request.nonce,
                                               readyTag: "final-ready", shutdownTag: "second-shutdown")
        try artifacts.write(request: request, first: first, mutation: mutation, final: final)
        try restoreSnapshot()
        return evidence
    }

    private func keyboardAndPointer() throws {
        try launchWorkload("KeyboardPointer")
        try ui.press("bridgevm.runtime.display.open", timeout: 10)
        try ui.clickSecondaryWindow(timeout: 15)
        try ui.setText("t17kbd\(prefix)", identifier: "bridgevm.runtime.keyboard.input", timeout: 10)
        try ui.press("bridgevm.runtime.keyboard.send", timeout: 10)
        try requireOutput("t17-keyboard-pointer-\(prefix).txt", timeout: 60)
    }

    private func clipboard() throws {
        let body = "브리지VM T17 클립보드 왕복 v1\n\(request.nonce)\n"
        let host = URL(fileURLWithPath: request.sharePath)
            .appendingPathComponent("t17-clipboard-host-\(prefix).txt")
        guard try Data(contentsOf: host) == Data(body.utf8) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "host clipboard challenge bytes changed")
        }
        let encoded = Data(body.utf8).base64EncodedString()
        try control("CLIPSET \(encoded)", requires: "OK CLIPSET")
        try control("CLIPGET", requires: body)
        try workload("Clipboard", output: "t17-clipboard-guest-\(prefix).txt")
    }

    private func workload(_ action: String, output: String, includeIdentity: Bool = false) throws {
        try launchWorkload(action, includeIdentity: includeIdentity)
        try requireOutput(output, timeout: 120)
    }

    private func launchWorkload(_ action: String, includeIdentity: Bool = false) throws {
        try waitForSharedAsset()
        var guest = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\bridgevm-share\\bv-product-e2e-launch.ps1 -Action \(action) -Nonce \(request.nonce)"
        if includeIdentity {
            guest += " -JobID \(request.jobID) -Commit \(request.commit) -Lane \(request.lane) -VMSlug \(request.vmSlug)"
        }
        try control(guest, requires: "T17-LAUNCHED-\(action)-\(prefix)")
    }

    private func control(_ command: String, requires marker: String) throws {
        let before = (try? Data(contentsOf: runLog).count) ?? 0
        try ui.setText(command, identifier: "bridgevm.runtime.ctl.input", timeout: 10)
        try ui.press("bridgevm.runtime.ctl.send", timeout: 10)
        guard wait(timeout: 120, predicate: {
            guard let data = try? Data(contentsOf: self.runLog), data.count > before else { return false }
            let tail = String(decoding: data.suffix(from: before), as: UTF8.self)
                .replacingOccurrences(of: "\r\n", with: "\n")
            return tail.contains(marker) && tail.contains("BVAGENT END")
        }) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest control command lacked its nonce-bound completion")
        }
    }

    private func shutdown() throws {
        let before = (try? Data(contentsOf: runLog).count) ?? 0
        try ui.setText("shutdown.exe /s /t 0 /f", identifier: "bridgevm.runtime.ctl.input", timeout: 10)
        try ui.press("bridgevm.runtime.ctl.send", timeout: 10)
        guard wait(timeout: 180, predicate: {
            guard let data = try? Data(contentsOf: self.runLog), data.count > before else { return false }
            return String(decoding: data.suffix(from: before), as: UTF8.self).contains("stop: PSCI SYSTEM_OFF")
        }), waitForStableLog() else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest did not reach clean SYSTEM_OFF")
        }
    }

    private func bootReady() throws -> String {
        guard !fileManager.fileExists(atPath: runLog.path) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "previous run log was not archived before boot")
        }
        try ui.press("bridgevm.windows.runtime.start", timeout: 30)
        var ready: String?
        guard wait(timeout: 600, predicate: {
            ready = self.currentLogText().split(whereSeparator: \.isNewline)
                .map(String.init).first(where: { $0.hasPrefix("BVAGENT READY") })
            return ready != nil
        }), let ready else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "boot did not produce a new BVAGENT READY record")
        }
        return ready
    }

    private func archiveLog(named name: String, ready: String, shutdown: String) throws -> T17RunLogProof {
        let proof = try T17RunLogProof.capture(runLog, nonce: request.nonce, readyTag: ready, shutdownTag: shutdown)
        let destination = evidenceRoot.appendingPathComponent(name)
        try fileManager.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.moveItem(at: runLog, to: destination)
        return proof
    }

    private func createSnapshot() throws {
        try ui.press("bridgevm.runtime.snapshot.create", timeout: 15)
        guard wait(timeout: 900, predicate: {
            (try? ui.text("bridgevm.runtime.snapshot.status", timeout: 1)) == "스냅샷 생성 완료"
        }) else { throw T17Blocker(code: "snapshot-unavailable", detail: "product UI did not complete snapshot creation") }
        try verifySnapshot(restored: false)
    }

    private func restoreSnapshot() throws {
        try ui.press("bridgevm.runtime.snapshot.restore", timeout: 15)
        guard wait(timeout: 900, predicate: {
            (try? ui.text("bridgevm.runtime.snapshot.status", timeout: 1)) == "스냅샷 복원 완료"
        }) else { throw T17Blocker(code: "snapshot-unavailable", detail: "product UI did not complete snapshot restore") }
        try verifySnapshot(restored: true)
    }

    private func verifySnapshot(restored: Bool) throws {
        let root = URL(fileURLWithPath: request.snapshotPath, isDirectory: true)
        let entries = try fileManager.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
        guard Set(entries.map(\.lastPathComponent)) == ["disk.raw", "vars.fd", "manifest.json"],
              let manifest = try JSONSerialization.jsonObject(
                with: Data(contentsOf: root.appendingPathComponent("manifest.json"))) as? [String: Any],
              Set(manifest.keys) == ["format_version", "vm_id", "disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256"] else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot pair or manifest shape is invalid")
        }
        let disk = root.appendingPathComponent("disk.raw"), vars = root.appendingPathComponent("vars.fd")
        let diskSize = try fileSize(disk), varsSize = try fileSize(vars)
        guard manifest["format_version"] as? Int == 1, manifest["vm_id"] as? String == request.vmSlug,
              manifest["disk_bytes"] as? Int == diskSize, manifest["vars_bytes"] as? Int == varsSize,
              manifest["disk_sha256"] as? String == (try T17Evidence.sha256(disk)),
              manifest["vars_sha256"] as? String == (try T17Evidence.sha256(vars)) else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot manifest does not authenticate its pair")
        }
        if restored {
            guard try T17Evidence.sha256(URL(fileURLWithPath: request.diskPath)) == T17Evidence.sha256(disk),
                  try T17Evidence.sha256(URL(fileURLWithPath: request.varsPath)) == T17Evidence.sha256(vars) else {
                throw T17Blocker(code: "snapshot-unavailable", detail: "restored media differs from the snapshot pair")
            }
        }
    }

    private func waitForSharedAsset() throws {
        guard wait(timeout: 120, predicate: {
            let log = self.currentLogText()
            return log.contains("BVAGENT SHARE host->guest bv-product-e2e-launch.ps1 bytes=")
                && log.contains("BVAGENT SHARE host->guest bv-product-e2e.ps1 bytes=")
        }) else { throw T17Blocker(code: "guest-evidence-missing", detail: "product E2E guest asset was not synchronized") }
    }

    private func waitForStableLog() -> Bool {
        var prior = -1, unchanged = 0
        return wait(timeout: 30, predicate: {
            let size = (try? Data(contentsOf: self.runLog).count) ?? -2
            unchanged = size == prior ? unchanged + 1 : 0
            prior = size
            return unchanged >= 5
        })
    }

    private func requireOutput(_ name: String, timeout: TimeInterval) throws {
        let url = URL(fileURLWithPath: request.sharePath).appendingPathComponent(name)
        guard wait(timeout: timeout, predicate: { self.fileManager.fileExists(atPath: url.path) }) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest workload did not produce \(name)")
        }
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size < 8 * 1024 * 1024 else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "guest output is unsafe or oversized")
        }
    }

    private func fileSize(_ url: URL) throws -> Int {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true, let size = values.fileSize else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot member is not a regular file")
        }
        return size
    }

    private var prefix: String { String(request.nonce.prefix(12)) }
    private var bundle: URL { URL(fileURLWithPath: request.libraryRootPath).appendingPathComponent(request.vmSlug).appendingPathComponent("bundle.vmbridge") }
    private var runLog: URL { bundle.appendingPathComponent("logs/hvf/run.log") }
    private var evidenceRoot: URL { bundle.appendingPathComponent("metadata/product-e2e", isDirectory: true) }
    private func currentLogText() -> String { (try? String(contentsOf: runLog, encoding: .utf8)) ?? "" }
    private func wait(timeout: TimeInterval, predicate: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if predicate() { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        return predicate()
    }
}
