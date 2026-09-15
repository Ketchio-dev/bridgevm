import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeWorkAdmissionIdentityTests: XCTestCase {
    func testRuntimeUsesImmutableSourceAndStaleGetterCannotReplaceCurrentSession() throws {
        let f = try HvfRuntimeWorkAdmissionFixture()
        defer { f.clean() }
        let original = f.config()
        f.save(original)
        let library = f.library()
        let old = try f.runtime(original, in: library)
        var options = old.config
        options.ramMiB = 8192; options.clipboardSync = false; options.nvmeBufferedIO = true
        XCTAssertTrue(old.acceptStartConfiguration(options))
        XCTAssertTrue(old.acceptStartConfiguration(options), "Unsaved options must not replace immutable source identity")
        XCTAssertTrue(try f.runtime(original, in: library) === old)
        XCTAssertNil(library.operationError)

        var changed = original
        changed.name = "saved rename"; changed.memMiB = 6144
        f.save(changed); library.reload()
        // The cache still holds old before a new getter. Public launch edits cannot rewrite its source snapshot.
        old.config = f.safeRuntimeConfig(try XCTUnwrap(HvfEngineConfig.libraryVM(changed, rootURL: f.root)))
        f.assertRuntimeRefused(old, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason,
            entries: [.configuration, .attach])
        old.config = options
        let fresh = try f.runtime(changed, in: library)
        XCTAssertFalse(fresh === old)
        XCTAssertTrue(try f.runtime(original, in: library) === fresh, "An obsolete getter must use current metadata")
        let bytes = try f.snapshot()
        f.assertRuntimeRefused(old, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
        XCTAssertEqual(old.config, options)
        XCTAssertTrue(fresh.acceptStartConfiguration(fresh.config))
        XCTAssertEqual(try f.snapshot(), bytes)
    }

    func testInstallSourceMetadataAndRequestReplacementDenyOldHandles() async throws {
        let f = try HvfRuntimeWorkAdmissionFixture()
        defer { f.clean() }
        let original = f.config(pending: true)
        f.save(original); f.saveRequest(original)
        let library = f.library()
        let old = f.install(original, in: library)
        let initial = old.start()
        XCTAssertNotNil(initial)
        if let initial { await initial.value }
        var changed = original
        changed.name = "changed display metadata"; changed.memMiB = 8192
        f.save(changed); library.reload()
        let fresh = f.install(changed, in: library)
        XCTAssertFalse(fresh === old, "Same bundle/request cannot erase changed accepted VMConfig")
        XCTAssertTrue(f.install(original, in: library) === fresh)
        let bytes = try f.snapshot()
        await f.assertInstallRefused(old, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
        XCTAssertEqual(try f.snapshot(), bytes)

        let current = fresh.start()
        XCTAssertNotNil(current)
        if let current { await current.value }
        XCTAssertEqual(fresh.stage, .failed("owned validation refusal"))
        f.saveRequest(changed, diskGiB: 65)
        let updatedRequest = f.install(changed, in: library)
        XCTAssertFalse(updatedRequest === fresh)
        XCTAssertEqual(updatedRequest.plan.request.diskGiB, 65)
        await f.assertInstallRefused(fresh, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
        XCTAssertEqual(f.installJobCount, 0)
    }

    func testBusyRetainedGenericHandleStaysDeniedUntilOrdinaryIdleReload() async throws {
        for operation in HvfRuntimeWorkAdmissionBackend.Operation.allCases {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let original = f.config()
            f.save(original)
            let library = f.library()
            let old = f.model(original, in: library)
            old.busy = true // Public observation setup; actual accepted work is covered separately.
            var changed = original
            changed.bundlePath = f.root.appendingPathComponent("new-bundle").path
            changed.memMiB = 6144
            f.save(changed); library.reload()
            old.busy = false
            XCTAssertTrue(f.model(original, in: library) === old)
            XCTAssertEqual(old.config, original)
            XCTAssertEqual(library.vms.first, changed)
            let bytes = try f.snapshot()

            await f.assertControlRefused(old, operation: operation, library: library,
                reason: HvfRuntimeWorkAdmissionFixture.ownerReason)

            library.reload()
            let fresh = f.model(original, in: library)
            XCTAssertFalse(fresh === old)
            XCTAssertEqual(fresh.config, changed, "A stale getter cannot seed an old configuration after reload")
            let work = HvfRuntimeWorkAdmissionOperation(.resources, model: fresh)
            let accepted = work.invoke()
            XCTAssertTrue(accepted)
            guard await f.acknowledge(work, accepted: accepted) else { continue }
            XCTAssertEqual(try f.snapshot(), bytes)
        }
    }

    func testRemovedAndEqualRecreatedRegistrationsRequireNewObjectIdentity() async throws {
        for kind in ["runtime", "install", "control"] {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config(pending: kind == "install")
            f.save(config)
            if kind == "install" { f.saveRequest(config) }
            let library = f.library()
            let oldRuntime = kind == "runtime" ? try f.runtime(config, in: library) : nil
            let oldInstall = kind == "install" ? f.install(config, in: library) : nil
            let oldModel = kind == "control" ? f.model(config, in: library) : nil
            try FileManager.default.removeItem(at: f.registration(config))
            library.reload()
            if let oldRuntime {
                f.assertRuntimeRefused(oldRuntime, library: library, reason: HvfRuntimeWorkAdmissionFixture.missingReason)
            }
            if let oldInstall {
                await f.assertInstallRefused(oldInstall, library: library, reason: HvfRuntimeWorkAdmissionFixture.missingReason)
            }
            if let oldModel {
                await f.assertControlRefused(oldModel, operation: .resources, library: library,
                    reason: HvfRuntimeWorkAdmissionFixture.missingReason)
            }
            f.save(config); library.reload()
            let bytes = try f.snapshot()
            if let oldRuntime {
                let fresh = try f.runtime(config, in: library)
                XCTAssertFalse(fresh === oldRuntime)
                f.assertRuntimeRefused(oldRuntime, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
                XCTAssertTrue(fresh.acceptStartConfiguration(fresh.config))
            }
            if let oldInstall {
                let fresh = f.install(config, in: library)
                XCTAssertFalse(fresh === oldInstall)
                await f.assertInstallRefused(oldInstall, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
                let handle = fresh.start()
                XCTAssertNotNil(handle)
                if let handle { await handle.value }
            }
            if let oldModel {
                let fresh = f.model(config, in: library)
                XCTAssertFalse(fresh === oldModel)
                await f.assertControlRefused(oldModel, operation: .resources, library: library,
                    reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
                let work = HvfRuntimeWorkAdmissionOperation(.resources, model: fresh)
                let accepted = work.invoke()
                XCTAssertTrue(accepted)
                guard await f.acknowledge(work, accepted: accepted) else { continue }
            }
            XCTAssertEqual(try f.snapshot(), bytes)
        }
    }
}
