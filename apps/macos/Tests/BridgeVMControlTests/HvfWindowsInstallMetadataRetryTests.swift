import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallMetadataRetryTests: XCTestCase {
    func testRepairThenRetryUsesCurrentRequestWithoutRecreatingVM() async throws {
        let f = try HvfWindowsInstallPlanPreparationFixture()
        defer { f.clean() }
        try await f.withAcknowledgedWork {
            let config = f.work.config(pending: true)
            f.save(config)
            try FileManager.default.removeItem(at: requestURL(config))
            let library = f.library()
            let host = try f.host(in: library), preparation = host.preparation
            await preparation.preparationTask?.value
            f.assertFailed(preparation)
            XCTAssertEqual(f.probe.snapshot.calls, 0)
            XCTAssertEqual(f.made, 0)
            let before = try f.work.snapshot()
            preparation.retry(); f.track(preparation)
            await preparation.preparationTask?.value
            f.assertFailed(preparation)
            XCTAssertEqual(f.probe.snapshot.calls, 0)
            XCTAssertEqual(try f.work.snapshot(), before)

            XCTAssertTrue(f.request(diskGiB: 96).save(bundlePath: config.bundlePath))
            preparation.retry(); f.track(preparation)
            await preparation.preparationTask?.value
            let session = try f.ready(preparation)
            XCTAssertTrue(try f.host(in: library).preparation === preparation)
            XCTAssertEqual(session.plan.request, f.request(diskGiB: 96))
            XCTAssertEqual(f.probe.snapshot.calls, 1)
            XCTAssertEqual(f.made, 1)
            XCTAssertEqual(f.jobCount, 0)
            XCTAssertEqual(library.vms.first { $0.slug == config.slug }, config)
            XCTAssertEqual(VMLibrary.scan(rootURL: f.work.root).configs.first { $0.slug == config.slug }, config)
        }
    }

    func testRequestRemovedDuringPreparationCannotPublishSession() async throws {
        let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
        defer { f.clean() }
        try await f.withAcknowledgedWork {
            let config = f.work.config(pending: true)
            f.save(config)
            let library = f.library(), host = try f.host(in: library)
            guard await f.entered(1) else { return }
            try FileManager.default.removeItem(at: requestURL(config))
            let before = try f.work.snapshot()
            f.probe.release(1)
            await host.preparation.preparationTask?.value
            f.assertFailed(host.preparation)
            XCTAssertEqual(f.made, 0)
            XCTAssertEqual(f.jobCount, 0)
            XCTAssertEqual(try f.work.snapshot(), before)
        }
    }

    func testActiveAcceptedInstallRetainsPlanAfterMetadataRemoval() async throws {
        let f = try HvfWindowsInstallPlanPreparationFixture(queuedInstall: true)
        defer { f.clean() }
        try await f.withAcknowledgedWork {
            let config = f.work.config(pending: true)
            f.save(config)
            let library = f.library(), host = try f.host(in: library)
            await host.preparation.preparationTask?.value
            let session = try f.ready(host.preparation), plan = session.plan
            try await f.withActive(session, queued: true) {
                try FileManager.default.removeItem(at: requestURL(config))
                let before = try f.work.snapshot()
                let retained = try f.host(in: library)
                XCTAssertTrue(try f.ready(retained.preparation) === session)
                XCTAssertEqual(session.plan, plan)
                XCTAssertEqual(f.probe.snapshot.calls, 1)
                XCTAssertEqual(f.made, 1)
                XCTAssertEqual(try f.work.snapshot(), before)
            }
        }
    }

    private func requestURL(_ config: VMConfig) -> URL {
        URL(fileURLWithPath: config.bundlePath).appendingPathComponent(HvfWindowsInstallRequest.fileName)
    }
}
