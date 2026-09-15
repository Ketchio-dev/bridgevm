import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeWorkAdmissionLifecycleTests: XCTestCase {
    func testExpiredLibraryRefusesRetainedRuntimeInstallAndGenericHandles() async throws {
        for kind in ["runtime", "install", "control-start", "control-resources"] {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config(pending: kind == "install")
            f.save(config)
            if kind == "install" { f.saveRequest(config) }
            var library: LibraryModel? = f.library()
            weak var owner = library
            let runtime = kind == "runtime" ? try f.runtime(config, in: XCTUnwrap(library)) : nil
            let install = kind == "install" ? f.install(config, in: try XCTUnwrap(library)) : nil
            let model = kind.hasPrefix("control") ? f.model(config, in: try XCTUnwrap(library)) : nil
            let bytes = try f.snapshot()
            library = nil
            XCTAssertNil(owner)

            if let runtime {
                XCTAssertEqual(runtime.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.expiredReason)
                f.assertRuntimeRefused(runtime, library: nil,
                    reason: HvfRuntimeWorkAdmissionFixture.expiredReason)
            } else if let install {
                XCTAssertEqual(install.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.expiredReason)
                await f.assertInstallRefused(install, library: nil,
                    reason: HvfRuntimeWorkAdmissionFixture.expiredReason)
            } else if let model {
                XCTAssertEqual(model.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.expiredReason)
                await f.assertControlRefused(model,
                    operation: kind == "control-start" ? .start : .resources,
                    library: nil, reason: HvfRuntimeWorkAdmissionFixture.expiredReason)
            }

            XCTAssertEqual(f.queue.count, 0)
            XCTAssertEqual(f.installJobCount, 0)
            XCTAssertEqual(try f.snapshot(), bytes)
        }
    }

    func testNilStandaloneCallbacksPreserveSafeBehaviorAndExperimentalIdentity() async throws {
        do {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config()
            f.save(config)
            let library = f.library()
            let bound = try f.runtime(config, in: library)
            let standalone = f.makeRuntime(bound.config)
            guard f.assertRuntimePathsAbsent(standalone) else { return }
            XCTAssertNil(standalone.workAdmission)
            var edited = standalone.config
            edited.ramMiB = 8192
            XCTAssertTrue(standalone.acceptStartConfiguration(edited))
            guard f.assertRuntimePathsAbsent(standalone) else { return }
            let bytes = try f.snapshot()
            standalone.start()
            XCTAssertEqual(standalone.connectionState, .stopped)
            XCTAssertFalse(standalone.events.isEmpty)
            XCTAssertFalse(standalone.attachToRunningVM(reportRefusal: false))
            XCTAssertFalse(standalone.attachIfStopped())
            XCTAssertEqual(f.access.processLookups, 3)
            XCTAssertEqual(f.access.keyRequests, 0)
            f.assertRuntimePathsAbsent(standalone)
            XCTAssertEqual(try f.snapshot(), bytes)

            // Default experimental paths are not an owned runtime fixture; inspect only identity/binding.
            let experimental = library.experimentalHvfRuntimeSession()
            XCTAssertTrue(library.experimentalHvfRuntimeSession() === experimental)
            XCTAssertNil(experimental.workAdmission)
            XCTAssertEqual(f.access.processLookups, 3)
        }
        do {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config(pending: true)
            f.save(config); f.saveRequest(config)
            let library = f.library()
            let plan = f.install(config, in: library).plan
            let standalone = HvfWindowsInstallSession(plan: plan,
                validate: { [probe = f.validator] in probe.validate($0) },
                schedule: { _ in XCTFail("The refusing validator must not schedule a pipeline") })
            XCTAssertNil(standalone.workAdmission)
            XCTAssertNil(standalone.onCompleted)
            let bytes = try f.snapshot()
            let calls = f.validator.snapshot.calls
            let handle = standalone.start()
            XCTAssertNotNil(handle)
            if let handle { await handle.value }
            XCTAssertEqual(standalone.stage, .failed("owned validation refusal"))
            XCTAssertEqual(f.validator.snapshot.calls, calls + 1)
            XCTAssertEqual(f.installJobCount, 0)
            XCTAssertEqual(try f.snapshot(), bytes)
        }
        for operation in HvfRuntimeWorkAdmissionBackend.Operation.allCases {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config()
            f.save(config)
            let model = ControlModel(config: config, backend: f.backend(config), startsAutomatically: false)
            XCTAssertNil(model.workAdmission)
            let bytes = try f.snapshot()
            let work = HvfRuntimeWorkAdmissionOperation(operation, model: model)
            let accepted = work.invoke()
            XCTAssertTrue(accepted)
            guard await f.acknowledge(work, accepted: accepted) else { continue }
            let calls = f.backend(config).calls
            XCTAssertEqual(calls.starts, operation == .start ? 1 : 0)
            XCTAssertEqual(calls.writes, operation == .resources ? 1 : 0)
            XCTAssertEqual(calls.stops, 0)
            XCTAssertEqual(calls.liveness, 0)
            XCTAssertEqual(try f.snapshot(), bytes)
        }
    }

    func testInstallCallbackAssignmentsPreserveRealBindingAndAccessorReload() async throws {
        let f = try HvfRuntimeWorkAdmissionFixture()
        defer { f.clean() }
        let config = f.config(pending: true)
        f.save(config); f.saveRequest(config)
        let library = f.library()
        let session = f.install(config, in: library)
        let admission = session.workAdmission
        XCTAssertNotNil(admission)
        XCTAssertNotNil(session.onCompleted)
        guard f.reserve(.move, config: config, in: library) else { return }
        let bytes = try f.snapshot()
        var completionCalls = 0

        session.onCompleted = { completionCalls += 1 }
        XCTAssertEqual(session.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.reservationReason)
        session.workAdmission = admission
        session.onCompleted?()
        XCTAssertEqual(completionCalls, 1)

        // No Start is invoked while either callback is temporarily cleared.
        session.workAdmission = nil
        XCTAssertNil(session.workAdmission)
        session.onCompleted?()
        XCTAssertEqual(completionCalls, 2)
        session.workAdmission = admission
        session.onCompleted = nil
        XCTAssertNil(session.onCompleted)
        XCTAssertEqual(session.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.reservationReason)
        session.onCompleted = { completionCalls += 1 }
        XCTAssertEqual(session.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.reservationReason)
        await f.assertInstallRefused(session, library: library,
            reason: HvfRuntimeWorkAdmissionFixture.reservationReason)

        let rebound = f.install(config, in: library)
        XCTAssertTrue(rebound === session)
        XCTAssertEqual(rebound.workAdmission?(false), HvfRuntimeWorkAdmissionFixture.reservationReason)
        XCTAssertNotNil(rebound.onCompleted)
        await f.assertInstallRefused(rebound, library: library,
            reason: HvfRuntimeWorkAdmissionFixture.reservationReason)
        XCTAssertEqual(try f.snapshot(), bytes)

        var changed = config
        changed.displayName = "Observed through the production completion callback"
        f.save(changed)
        XCTAssertEqual(library.vms, [config])
        let changedBytes = try f.snapshot()
        rebound.onCompleted?() // Callback wiring only; no installation/finalization is executed.
        XCTAssertEqual(library.vms, [changed])
        XCTAssertEqual(completionCalls, 2)
        XCTAssertEqual(f.queue.count, 1)
        XCTAssertEqual(f.installJobCount, 0)
        XCTAssertEqual(try f.snapshot(), changedBytes)
    }
}
