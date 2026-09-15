import XCTest
import Combine
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPlanPreparationIdentityTests: XCTestCase {
    func testChangedOrMissingRegistrationDiscardsCompletion() async throws {
        for change in ["name", "resources", "bundle", "ready", "backend", "removed"] {
            let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                let original = f.work.config(pending: true)
                f.save(original)
                let library = f.library()
                let old = try f.host(in: library).preparation
                guard await f.entered(1) else { return }
                var current = original
                switch change {
                case "name": current.displayName = "new accepted display name"
                case "resources": current.memMiB = try XCTUnwrap(current.memMiB) + 1024
                case "bundle": current.bundlePath = f.work.root.appendingPathComponent("replacement/bundle").path
                case "ready": current.installPending = false
                case "backend": current.backendKind = "fast-vz"
                default: break
                }
                if change == "removed" { try FileManager.default.removeItem(at: f.work.registration(original)) }
                else { f.save(current) }
                library.reload()
                let bytes = try f.work.snapshot()
                f.probe.release(1)
                await old.preparationTask?.value
                f.assertFailed(old)
                XCTAssertEqual(f.made, 0, "A changed registration cannot publish the old Plan")
                XCTAssertEqual(f.probe.snapshot.inputs.first?.config, original)
                XCTAssertEqual(try f.work.snapshot(), bytes)
                let latest = f.prepare(original, in: library) // A stale caller cannot bless its old config.
                if ["ready", "backend", "removed"].contains(change) {
                    f.assertFailed(latest)
                    XCTAssertTrue(f.prepare(original, in: library) === latest)
                    XCTAssertEqual(f.probe.snapshot.calls, 1)
                } else {
                    await latest.preparationTask?.value
                    XCTAssertFalse(latest === old)
                    XCTAssertEqual(f.probe.snapshot.inputs.last?.config, current)
                    XCTAssertEqual(try f.ready(latest).plan.bundlePath, current.bundlePath)
                    XCTAssertEqual(library.windowsInstallSessions.record(for: current.slug)?.sourceConfig, current)
                }
            }
        }
    }

    func testChangedRequestDiscardsCompletionWithoutReload() async throws {
        for sealed in [true, false] {
            let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                let config = f.work.config(pending: true)
                f.save(config, sealed: sealed)
                let library = f.library()
                let preparation = try f.host(in: library).preparation
                guard await f.entered(1) else { return }
                XCTAssertTrue(f.request(sealed: sealed, diskGiB: 96).save(bundlePath: config.bundlePath))
                let bytes = try f.work.snapshot()
                f.probe.release(1)
                await preparation.preparationTask?.value
                f.assertFailed(preparation)
                XCTAssertEqual(f.made, 0)
                XCTAssertEqual(library.vms.first, config)
                let oldAttempt = preparation.attemptID
                preparation.retry()
                f.track(preparation)
                XCTAssertNotEqual(preparation.attemptID, oldAttempt)
                await preparation.preparationTask?.value
                XCTAssertTrue(f.prepare(config, in: library) === preparation)
                XCTAssertEqual(try f.ready(preparation).plan.request, f.request(sealed: sealed, diskGiB: 96))
                XCTAssertEqual(f.probe.snapshot.inputs.map(\.request.diskGiB), [64, 96])
                XCTAssertEqual(f.made, 1)
                XCTAssertEqual(try f.work.snapshot(), bytes)
            }
        }
    }

    func testNewGenerationRejectsLateCompletionIncludingABAInputs() async throws {
        do {
            let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1, 2, 3])
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                let config = f.work.config(pending: true)
                f.save(config)
                let library = f.library()
                let a1 = try f.host(in: library).preparation
                guard await f.entered(1) else { return }
                XCTAssertTrue(f.request(diskGiB: 96).save(bundlePath: config.bundlePath))
                let b = f.prepare(config, in: library)
                guard await f.entered(2) else { return }
                XCTAssertTrue(f.request().save(bundlePath: config.bundlePath))
                let a2 = f.prepare(config, in: library)
                guard await f.entered(3) else { return }
                XCTAssertFalse(a1 === b)
                XCTAssertFalse(a1 === a2)
                XCTAssertFalse(b === a2)
                XCTAssertEqual(f.probe.snapshot.inputs[0], f.probe.snapshot.inputs[2])
                let bytes = try f.work.snapshot()
                f.probe.release(3)
                await a2.preparationTask?.value
                let winner = try f.ready(a2)
                let task = a2.preparationTask, attempt = a2.attemptID
                f.probe.release(2); f.probe.release(1)
                await b.preparationTask?.value
                await a1.preparationTask?.value
                XCTAssertTrue(f.prepare(config, in: library) === a2)
                XCTAssertTrue(try f.ready(a2) === winner)
                XCTAssertEqual(a2.attemptID, attempt)
                XCTAssertEqual(a2.preparationTask, task)
                XCTAssertEqual(f.made, 1)
                XCTAssertEqual(try f.work.snapshot(), bytes)
            }
        }
        do {
            let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1, 2])
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                var config = f.work.config(pending: true)
                f.save(config)
                let library = f.library()
                let preparation = try f.host(in: library).preparation
                guard await f.entered(1) else { return }
                let oldTask = try XCTUnwrap(preparation.preparationTask)
                config.displayName = "retry current registration"
                f.save(config); library.reload() // Reconcile invalidates this still-indexed handle.
                f.assertFailed(preparation)
                let invalidatedAttempt = preparation.attemptID
                preparation.retry(); f.track(preparation)
                let currentTask = try XCTUnwrap(preparation.preparationTask)
                guard await f.entered(2) else { return }
                XCTAssertNotEqual(preparation.attemptID, invalidatedAttempt)
                let currentAttempt = preparation.attemptID
                currentTask.cancel()
                f.probe.release(1)
                await oldTask.value
                f.assertPreparing(preparation)
                XCTAssertEqual(preparation.attemptID, currentAttempt)
                XCTAssertEqual(preparation.preparationTask?.isCancelled, true)
                f.probe.release(2)
                await currentTask.value
                XCTAssertTrue(f.prepare(config, in: library) === preparation)
                XCTAssertEqual(library.windowsInstallSessions.record(for: config.slug)?.sourceConfig, config)
                _ = try f.ready(preparation)
                XCTAssertEqual(f.made, 1)
            }
        }
    }

    func testCurrentAcceptedSessionWinsOverLatePlanCompletion() async throws {
        for mode in ["idle-other-repo", "validating", "queued"] {
            for lookupFirst in [false, true] {
                let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1], queuedInstall: mode == "queued")
                defer { f.clean() }
                try await f.withAcknowledgedWork {
                    let config = f.work.config(pending: true)
                    f.save(config)
                    let library = f.library()
                    let old = try f.host(in: library).preparation
                    guard await f.entered(1) else { return }
                    let repo = mode == "idle-other-repo" ? try f.alternateRepo() : f.work.installRepo
                    let winner = library.windowsInstallSession(for: config, repoRoot: repo)
                    XCTAssertEqual(winner.plan.repoRoot, repo)
                    let checkWinner: @MainActor () async throws -> Void = {
                        var publications = 0
                        let subscription = old.objectWillChange.sink { publications += 1 }
                        defer { subscription.cancel() }
                        let bytes = try f.work.snapshot()
                        let host: HvfWindowsInstallPreparationView
                        if lookupFirst {
                            host = try f.host(in: library)
                            XCTAssertFalse(host.preparation === old)
                            try f.assertReadyHost(host, session: winner) // Immediate cache adoption.
                            XCTAssertEqual(publications, 0)
                            f.probe.release(1)
                            await old.preparationTask?.value
                        } else {
                            // Keep the old handle indexed so completion must apply cache precedence itself.
                            f.probe.release(1)
                            await old.preparationTask?.value
                            XCTAssertTrue(try f.ready(old) === winner)
                            let beforeLookup = publications
                            host = try f.host(in: library)
                            XCTAssertTrue(host.preparation === old)
                            try f.assertReadyHost(host, session: winner)
                            XCTAssertEqual(publications, beforeLookup)
                        }
                        XCTAssertEqual(f.probe.snapshot.calls, 1)
                        XCTAssertEqual(f.made, 1)
                        XCTAssertTrue(try f.host(in: library).preparation === host.preparation)
                        XCTAssertTrue(try f.ready(host.preparation) === winner)
                        XCTAssertTrue(library.windowsInstallSessions.record(for: config.slug)?.session === winner)
                        XCTAssertEqual(try f.work.snapshot(), bytes)
                    }
                    if mode == "idle-other-repo" { try await checkWinner() }
                    else { try await f.withActive(winner, queued: mode == "queued", during: checkWinner) }
                }
            }
        }
        do {
            let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                let config = f.work.config(pending: true)
                f.save(config)
                let library = f.library()
                let preparation = try f.host(in: library).preparation
                guard await f.entered(1) else { return }
                XCTAssertTrue(f.request(diskGiB: 96).save(bundlePath: config.bundlePath))
                let incompatible = library.windowsInstallSession(for: config, repoRoot: f.work.installRepo)
                XCTAssertTrue(f.request().save(bundlePath: config.bundlePath)) // Restore A, retaining the new B cache object.
                let bytes = try f.work.snapshot()
                f.probe.release(1)
                await preparation.preparationTask?.value
                f.assertFailed(preparation)
                XCTAssertTrue(f.prepare(config, in: library) === preparation)
                XCTAssertTrue(library.windowsInstallSessions.record(for: config.slug)?.session === incompatible)
                XCTAssertEqual(incompatible.plan.request.diskGiB, 96)
                XCTAssertEqual(incompatible.stage, .idle)
                XCTAssertEqual(f.probe.snapshot.calls, 1)
                XCTAssertEqual(f.made, 1)
                XCTAssertEqual(try f.work.snapshot(), bytes)
            }
        }
    }
}
