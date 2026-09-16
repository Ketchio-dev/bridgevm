import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeStatusLibraryTests: XCTestCase {
    func testAbsentLookupDoesNotConstructAControlOrRuntime() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config()
        f.work.save(config)
        let library = f.work.library()
        let effects = f.effects, bytes = try f.work.snapshot(), selection = library.selectedID
        for slug in [config.slug, "missing"] {
            XCTAssertNil(library.hvfRuntimeSessions.existingRecord(slug: slug))
            XCTAssertEqual(try library.runtimeObservations(slug: slug, requestedConfigurationIdentity: nil), [])
        }
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(try f.work.snapshot(), bytes)
        XCTAssertEqual(library.selectedID, selection)
    }

    func testFreshSessionIsUnobservedAndDigestUsesAcceptedSourceConfiguration() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config()
        f.work.save(config)
        let library = f.work.library(), session = try f.work.runtime(config, in: library)
        let accepted = try NativeRuntimeConfigurationIdentity.digest(config: config)
        var changed = config
        changed.memMiB = 8192
        let changedDigest = try NativeRuntimeConfigurationIdentity.digest(config: changed)
        session.config.ramMiB = 16384 // UI launch options do not replace the retained source config.
        session.events = [.unknown("preserved event")]
        session.lastHeartbeatAge = 42
        let effects = f.effects, bytes = try f.work.snapshot(), snapshot = session.runtimeObservation()
        let comparisons: [(String?, NativeRuntimeSessionObservation.ConfigurationMatch)] = [
            (accepted, .same), (changedDigest, .different), (nil, .unknown)]
        for (requested, match) in comparisons {
            let values = try library.runtimeObservations(slug: config.slug, requestedConfigurationIdentity: requested)
            XCTAssertEqual(values.count, 1)
            let value = try XCTUnwrap(values.first)
            XCTAssertEqual(value.ownership, .notObserved)
            XCTAssertNil(value.connectionState, "Fresh stopped is not evidence of any process exit")
            XCTAssertNil(value.ownedProcess)
            XCTAssertNil(value.lastOwnedExit)
            XCTAssertEqual(value.acceptedConfigurationDigest, accepted)
            XCTAssertEqual(value.configurationMatch, match)
        }
        XCTAssertEqual(session.runtimeObservation(), snapshot)
        XCTAssertEqual(session.events, [.unknown("preserved event")])
        XCTAssertEqual(session.lastHeartbeatAge, 42)
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }

    func testRemovedRegistrationDeduplicatesActiveHandleAndKeepsPrunedTerminalHandle() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config()
        f.work.save(config)
        let library = f.work.library(), session = try f.work.runtime(config, in: library)
        session.connectionState = .booting
        try f.removeRegistration(config)
        library.reload()
        XCTAssertEqual(library.retainedControlRecords.count, 1)
        XCTAssertTrue(library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session === session)
        let before = f.effects, bytes = try f.work.snapshot()
        let active = try library.runtimeObservations(slug: config.slug, requestedConfigurationIdentity: nil)
        XCTAssertEqual(active.count, 1, "The same store and retained-record object must appear only once")
        XCTAssertEqual(active.first?.configurationMatch, .unknown)
        XCTAssertEqual(f.effects, before)
        XCTAssertEqual(try f.work.snapshot(), bytes)
        session.connectionState = .stopped
        library.reload()
        XCTAssertNil(library.hvfRuntimeSessions.existingRecord(slug: config.slug))
        let afterPruning = f.effects
        let terminal = try library.runtimeObservations(slug: config.slug, requestedConfigurationIdentity: nil)
        XCTAssertEqual(terminal, active)
        XCTAssertEqual(terminal.first?.ownership, .notObserved, "A synthetic state change cannot invent an exit")
        XCTAssertEqual(f.effects, afterPruning)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }

    func testRecreatedRegistrationReportsBothAcceptedConfigurations() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let oldConfig = f.work.config()
        f.work.save(oldConfig)
        let library = f.work.library(), old = try f.work.runtime(oldConfig, in: library)
        old.connectionState = .booting
        try f.removeRegistration(oldConfig)
        library.reload()
        old.connectionState = .stopped
        library.reload()
        var newConfig = oldConfig
        newConfig.memMiB = 8192
        f.work.save(newConfig)
        library.reload()
        let current = try f.work.runtime(newConfig, in: library)
        XCTAssertFalse(current === old)
        let requested = try NativeRuntimeConfigurationIdentity.digest(config: newConfig)
        let effects = f.effects, bytes = try f.work.snapshot()
        let result = try library.runtimeObservations(slug: oldConfig.slug, requestedConfigurationIdentity: requested)
        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(result.map(\.configurationMatch), [.same, .different])
        XCTAssertEqual(Set(result.compactMap(\.acceptedConfigurationDigest)).count, 2)
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }

    func testSessionLimitRefusesOverflowWithoutTruncatingOrConstructingWork() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config(), other = f.work.config("other")
        let library = f.work.library()
        let makeRecord: (VMConfig) throws -> LibraryRetainedControlDescriptor = { config in
            let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: f.work.root))
            let session = f.work.makeRuntime(launch)
            session.connectionState = .booting
            return .runtime(config: config, session: session)
        }
        library.retainedControlStore.capture([try makeRecord(other)])
        library.retainedControlStore.capture(try (0..<32).map { _ in try makeRecord(config) })
        let effects = f.effects, bytes = try f.work.snapshot()
        XCTAssertEqual(try library.runtimeObservations(slug: config.slug, requestedConfigurationIdentity: nil).count, 32)
        library.retainedControlStore.capture([try makeRecord(config)])
        XCTAssertThrowsError(try library.runtimeObservations(slug: config.slug, requestedConfigurationIdentity: nil)) {
            XCTAssertEqual($0 as? NativeRuntimeError, .snapshotUnavailable)
        }
        XCTAssertEqual(try library.runtimeObservations(slug: other.slug, requestedConfigurationIdentity: nil).count, 1)
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }
}
