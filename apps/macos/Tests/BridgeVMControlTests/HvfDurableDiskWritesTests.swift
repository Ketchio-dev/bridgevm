import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfDurableDiskWritesTests: XCTestCase {
    private func ownedArguments(_ config: HvfEngineConfig) -> [String] {
        config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p", ownedRuntime: true)
    }

    /// Ordered flushes are the default; only the user's saved choice launches with durable ones.
    func testDurableDiskWritesAreOffUntilTheUserSavesThem() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertFalse(session.config.durableDiskWrites, "An absent key keeps ordered flushes")
        XCTAssertFalse(ownedArguments(session.config).contains("--nvme-durable-flush"))
        XCTAssertFalse(session.config.wrapperArguments().contains("--nvme-durable-flush"))

        XCTAssertTrue(session.applyDurableDiskWritesToggle(true) == (true, nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.durableDiskWritesEnabled, true)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        XCTAssertTrue(ownedArguments(launch).contains("--nvme-durable-flush"))
        XCTAssertTrue(launch.wrapperArguments().contains("--nvme-durable-flush"))

        // Turning it off again restores the never-touched registration.
        XCTAssertTrue(session.applyDurableDiskWritesToggle(false) == (false, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).durableDiskWritesEnabled)
    }
}
