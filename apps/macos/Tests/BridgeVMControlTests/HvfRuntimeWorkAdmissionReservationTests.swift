import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeWorkAdmissionReservationTests: XCTestCase {
    func testEveryFileReservationRejectsEachRuntimeEntryBeforeEffects() throws {
        for action in HvfRuntimeLibraryActionEntry.finalActions {
            for entry in HvfRuntimeWorkAdmissionEntry.allCases {
                let f = try HvfRuntimeWorkAdmissionFixture()
                defer { f.clean() }
                let config = f.config()
                f.save(config)
                let library = f.library()
                let session = try f.runtime(config, in: library)
                session.events = [.unknown("retained observation")]
                session.lastHeartbeatAge = 7
                guard f.reserve(action, config: config, in: library) else { continue }
                f.seedErrors(library)
                let bytes = try f.snapshot()

                f.assertRuntimeRefused(session, library: library,
                    reason: HvfRuntimeWorkAdmissionFixture.reservationReason, entries: [entry])

                f.assertFileErrors(library)
                XCTAssertEqual(f.queue.count, 1)
                XCTAssertEqual(try f.snapshot(), bytes)
            }
        }
    }

    func testEveryFileReservationRejectsInstallBeforeValidationOrHistoryReset() async throws {
        for action in HvfRuntimeLibraryActionEntry.finalActions {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config(pending: true)
            f.save(config); f.saveRequest(config)
            let library = f.library()
            let session = f.install(config, in: library)
            session.cancel() // A real idle entry seeds an existing log without any process.
            guard f.reserve(action, config: config, in: library) else { continue }
            f.seedErrors(library)
            let bytes = try f.snapshot()

            await f.assertInstallRefused(session, library: library,
                reason: HvfRuntimeWorkAdmissionFixture.reservationReason)

            f.assertFileErrors(library)
            XCTAssertEqual(f.queue.count, 1)
            XCTAssertEqual(try f.snapshot(), bytes)
        }
    }

    func testEveryFileReservationRejectsGenericStartAndResourcesBeforeBackendCalls() async throws {
        for action in HvfRuntimeLibraryActionEntry.finalActions {
            for operation in HvfRuntimeWorkAdmissionBackend.Operation.allCases {
                let f = try HvfRuntimeWorkAdmissionFixture()
                defer { f.clean() }
                let config = f.config()
                f.save(config)
                let library = f.library()
                let model = f.model(config, in: library)
                model.statusNote = "retained status"
                model.pendingMemGiB = 6; model.pendingCPU = 3
                guard f.reserve(action, config: config, in: library) else { continue }
                f.seedErrors(library)
                let bytes = try f.snapshot()

                await f.assertControlRefused(model, operation: operation, library: library,
                    reason: HvfRuntimeWorkAdmissionFixture.reservationReason)

                f.assertFileErrors(library)
                XCTAssertEqual(f.queue.count, 1)
                XCTAssertEqual(try f.snapshot(), bytes)
            }
        }
    }

    func testOtherSlugsAndRootsAdmitSafeWorkWithoutClearingExistingErrors() async throws {
        let f = try HvfRuntimeWorkAdmissionFixture(), otherRoot = try HvfRuntimeWorkAdmissionFixture()
        defer { f.clean(); otherRoot.clean() }
        let reserved = f.config(), runtimeConfig = f.config("runtime")
        let installConfig = f.config("install", pending: true), controlConfig = f.config("control")
        for config in [reserved, runtimeConfig, installConfig, controlConfig] { f.save(config) }
        f.saveRequest(installConfig)
        let library = f.library()
        guard f.reserve(.move, config: reserved, in: library) else { return }
        f.seedErrors(library)
        let runtime = try f.runtime(runtimeConfig, in: library)
        var edited = runtime.config
        edited.ramMiB = 8192
        XCTAssertTrue(runtime.acceptStartConfiguration(edited))
        XCTAssertFalse(runtime.attachIfStopped())
        XCTAssertEqual(f.access.processLookups, 1)

        let install = f.install(installConfig, in: library)
        let handle = install.start()
        XCTAssertNotNil(handle)
        if let handle { await handle.value }
        XCTAssertEqual(install.stage, .failed("owned validation refusal"))
        XCTAssertEqual(f.installJobCount, 0)
        let model = f.model(controlConfig, in: library)
        let operation = HvfRuntimeWorkAdmissionOperation(.resources, model: model)
        let accepted = operation.invoke()
        XCTAssertTrue(accepted)
        guard await f.acknowledge(operation, accepted: accepted) else { return }
        XCTAssertEqual(f.backend(controlConfig).calls.writes, 1)
        XCTAssertEqual(library.operationError, "existing operation notice")
        f.assertFileErrors(library)

        var sameSlug = reserved
        sameSlug.bundlePath = otherRoot.root.appendingPathComponent("same-slug/bundle").path
        otherRoot.save(sameSlug)
        let otherLibrary = otherRoot.library()
        otherRoot.seedErrors(otherLibrary)
        let otherRuntime = try otherRoot.runtime(sameSlug, in: otherLibrary)
        let bytes = try otherRoot.snapshot()
        otherRuntime.start() // Real stopped Start and its nested attach reach only owned readiness blockers.
        XCTAssertEqual(otherRoot.access.processLookups, 1)
        XCTAssertFalse(otherRuntime.events.isEmpty)
        XCTAssertEqual(otherRuntime.connectionState, .stopped)
        XCTAssertEqual(otherLibrary.operationError, "existing operation notice")
        otherRoot.assertFileErrors(otherLibrary)
        otherRoot.assertRuntimePathsAbsent(otherRuntime)
        XCTAssertEqual(try otherRoot.snapshot(), bytes)
        XCTAssertEqual(f.queue.count, 1)
        XCTAssertEqual(otherRoot.queue.count, 0)
    }
}
