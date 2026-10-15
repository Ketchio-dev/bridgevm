#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Darwin

@MainActor
final class AppUIInstallCancelCapture {
    static let names = ["install-idle", "install-validating", "install-cancelling", "install-cancelled", "install-retry-failed"]
    let output: URL
    let fixture: AppUIInstallCancelFixture
    private var screenshots: [[String: Any]] = []
    private var checkpoints: [[String: Any]] = []
    private var failure: String?
    private var saved = false

    init(output: URL, fixture: AppUIInstallCancelFixture) {
        self.output = output; self.fixture = fixture
    }

    func capture(_ phase: AppUIInstallCancelPhase, window: NSWindow, content: NSView,
                 readSequence: UInt32) throws {
        guard let index = AppUIInstallCancelPhase.allCases.firstIndex(of: phase), index == screenshots.count,
              window.isVisible, window.contentView === content, content.window === window,
              readSequence > 0, try fixture.matches(phase), let session = fixture.session else {
            throw AppUIDriverFailure.invalidReply
        }
        content.layoutSubtreeIfNeeded(); content.displayIfNeeded()
        let pixels = content.convertToBacking(content.bounds)
        guard pixels.width.isFinite, pixels.height.isFinite, pixels.width > 0, pixels.height > 0,
              pixels.width <= 8192, pixels.height <= 8192 else { throw AppUIDriverFailure.protocolLimit }
        guard let bitmap = content.bitmapImageRepForCachingDisplay(in: content.bounds),
              bitmap.pixelsWide > 0, bitmap.pixelsHigh > 0,
              bitmap.pixelsWide <= 8192, bitmap.pixelsHigh <= 8192 else { throw AppUIDriverFailure.protocolLimit }
        content.effectiveAppearance.performAsCurrentDrawingAppearance { content.cacheDisplay(in: content.bounds, to: bitmap) }
        guard let png = bitmap.representation(using: .png, properties: [:]), !png.isEmpty,
              png.count <= 32 * 1024 * 1024 else { throw AppUIDriverFailure.protocolLimit }
        let name = Self.names[index]
        try png.write(to: output.appendingPathComponent(name + ".png"), options: .withoutOverwriting)
        screenshots.append(["name": name, "file": name + ".png", "sha256": AppUIHostCapture.digest(png),
            "width": bitmap.pixelsWide, "height": bitmap.pixelsHigh])
        let counts = fixture.counters.snapshot
        checkpoints.append(["phase": phase.rawValue, "stage": phase == .retryFailed ? "failed" : phase.rawValue,
            "is_running": session.isRunning, "can_cancel": session.canCancel,
            "validator_calls": counts["validation_calls"]!, "validator_finished": counts["validation_finished"]!,
            "process_resets": counts["process_resets"]!, "process_cancels": counts["process_cancels"]!,
            "fixture_preserved": true, "read_sequence": readSequence])
        try save()
    }

    func save() throws {
        let files: [[String: Any]] = fixture.files.map {
            ["path": "fixture-library/" + $0.path, "bytes": $0.bytes, "sha256": $0.sha256]
        }
        let record: [String: Any] = ["schema_version": 1, "kind": "native-app-ui-install-cancel-observations",
            "fixture_data": true, "guest_behavior_proof": false, "screenshots": screenshots,
            "checkpoints": checkpoints, "counters": fixture.counters.snapshot, "fixture_files": files,
            "failure": failure.map { $0 as Any } ?? NSNull()]
        let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        guard data.count <= 65_536 else { throw AppUIDriverFailure.protocolLimit }
        try data.write(to: output.appendingPathComponent("install-cancel-observations.json"),
                       options: saved ? .atomic : .withoutOverwriting)
        saved = true
    }

    func finish(error: Error?) async {
        if let error { failure = String(String(describing: error).prefix(512)) }
        let settled = await fixture.settle()
        let preserved = (try? fixture.preserved()) == true
        let complete = screenshots.count == 5 && checkpoints.count == 5
            && (try? fixture.matches(.retryFailed)) == true
        if !settled || !preserved || !complete { failure = failure ?? "Synthetic install UI did not complete and settle" }
        do {
            try save()
            let report = output.appendingPathComponent("install-cancel-observations.json")
            let completion = AppUIInstallCancelHostCompletion(pid: getpid(), success: failure == nil,
                settled: settled, fixturePreserved: preserved, reportSHA256: try AppUIHostCapture.digest(report),
                failureCode: failure == nil ? nil : .internalFailure)
            try AppUIInstallCancelMailbox(privateRoot: output.deletingLastPathComponent()).publishHostCompletion(completion)
        } catch {
            FileHandle.standardError.write(Data("Install cancellation diagnostic completion refused.\n".utf8))
        }
    }
}
#endif
