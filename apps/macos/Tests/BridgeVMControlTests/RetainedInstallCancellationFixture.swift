import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// Actual session pipeline and retained routing, with every installer effect
/// injected. Deferred windows are never shown, ordered front or focused.
@MainActor
final class RetainedInstallCancellationFixture {
    let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("retained-install-" + UUID().uuidString)
    let gate = HvfWindowsInstallCancellationGate()
    let queue = HvfWindowsInstallPipelineQueue()
    let buttons = InstallCancellationButtonProbe()
    let finalizing: Bool
    var preparations = 0, finalizations = 0
    lazy var process = HvfWindowsInstallCancellationProcess(gate: gate, holdProcess: !finalizing)
    lazy var config = VMConfig(id: "pending", name: "Pending", displayName: "Pending", backendKind: "hvf-engine",
        bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("bundle").path,
        runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
        leasesPath: "", guestName: "pending", displayWidth: 1280, displayHeight: 720, installPending: true)
    lazy var library = LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { [unowned self] plan in
        let execution = HvfWindowsInstallExecution(process: process, verifySource: { _ in true },
            prepareMedia: { [unowned self] _ in preparations += 1 },
            finalize: { [unowned self] _ in finalizations += 1; if finalizing { await gate.hold() } })
        return HvfWindowsInstallSession(plan: plan, validate: { _ in nil }, schedule: queue.enqueue,
            execution: execution, recovery: .init(inspect: { _ in .fresh }))
    }, startsModelsAutomatically: false)
    var repo: URL { root.appendingPathComponent("repo") }

    init(finalizing: Bool) throws {
        self.finalizing = finalizing
        let helpers = repo.appendingPathComponent("helpers")
        try FileManager.default.createDirectory(at: helpers, withIntermediateDirectories: true)
        for name in ["wimlib-imagex", "bridgevm-catalog-verify"] {
            let path = helpers.appendingPathComponent(name)
            try Data("owned marker; never executed\n".utf8).write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: path.path)
        }
        XCTAssertEqual(HvfWindowsWimlib.resolve(repoRoot: repo), helpers.appendingPathComponent("wimlib-imagex").path)
        XCTAssertEqual(HvfWindowsCatalogVerifier.resolve(repoRoot: repo), helpers.appendingPathComponent("bridgevm-catalog-verify").path)
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: config.bundlePath))
    }

    func run(_ body: (HvfWindowsInstallSession, NSHostingView<AnyView>) async throws -> Void) async throws {
        let session = library.windowsInstallSession(for: config, repoRoot: repo)
        let accepted = try XCTUnwrap(session.start())
        await accepted.value
        XCTAssertEqual(queue.jobs.count, 1)
        guard !queue.jobs.isEmpty else { return }
        let worker = Task { await queue.runNext() }
        let entered = await gate.waitForEntry()
        XCTAssertTrue(entered)
        do {
            if entered {
                try FileManager.default.removeItem(at: root.appendingPathComponent(config.slug + "/vm.json"))
                library.reload()
                let record = try XCTUnwrap(library.retainedControlRecords.first)
                library.selectedID = record.id
                let detail = try XCTUnwrap(values(RetainedControlDetailView.self,
                    in: LibraryDetailView(library: library).body).first)
                let retained = try XCTUnwrap(values(RetainedInstallControlView.self, in: detail.body).first)
                XCTAssertTrue(retained.session === session)
                let host = NSHostingView(rootView: AnyView(detail.buttonStyle(InstallCancellationProbeStyle(probe: buttons))))
                let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 800, height: 600),
                    styleMask: [], backing: .buffered, defer: true)
                window.contentView = host
                defer { window.contentView = nil }
                await settle(host)
                try await body(session, host)
            }
        } catch { gate.release(); await worker.value; throw error }
        gate.release()
        await worker.value
        XCTAssertFalse(gate.timedOut)
        XCTAssertFalse(session.isRunning)
    }

    func settle(_ view: NSView) async {
        for _ in 0..<5 { await Task.yield() }
        view.layoutSubtreeIfNeeded()
    }

    func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    func clean() {
        gate.release(); queue.discard()
        try? FileManager.default.removeItem(at: root)
    }
}
