import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeLibraryActionAdmissionTests: XCTestCase {
    private typealias Fixture = HvfRuntimeLibraryActionFixture
    private typealias Entry = HvfRuntimeLibraryActionEntry

    func testAllOrderedFileActionReservationsRefuseAnotherAction() throws {
        for first in Entry.finalActions {
            for second in Entry.finalActions {
                let fixture = Fixture()
                defer { fixture.clean() }
                let config = fixture.config()
                fixture.save(config)
                let before = try fixture.snapshot()
                let library = fixture.library()
                fixture.cacheFake(in: library, config: config)

                fixture.assertAccepted(first, config: config, library: library)
                fixture.assertRefused([.requestDeletion, .requestClone], config: config, library: library)
                fixture.assertRefused([second], config: config, library: library)

                XCTAssertEqual(fixture.queue.count, 1)
                XCTAssertEqual(try fixture.snapshot(), before)
            }
        }
    }

    func testEveryNonstoppedRuntimeOwnerRefusesRequestsAndFinalActions() throws {
        let states: [HvfConnectionState] = [.booting, .connected(host: "fixture"), .stopping, .timedOut]
        for state in states {
            let fixture = Fixture()
            defer { fixture.clean() }
            let config = fixture.config()
            fixture.save(config)
            let before = try fixture.snapshot()
            let library = fixture.library()
            fixture.cacheFake(in: library, config: config)
            let session = try XCTUnwrap(library.hvfRuntimeSession(for: config))
            session.connectionState = state
            let accepted = session.config

            fixture.assertRefused(config: config, library: library)

            XCTAssertEqual(fixture.queue.count, 0)
            XCTAssertEqual(session.connectionState, state)
            XCTAssertEqual(session.config, accepted)
            XCTAssertTrue(library.hvfRuntimeSession(for: config) === session)
            XCTAssertEqual(try fixture.snapshot(), before)
        }
    }

    func testValidatingAndQueuedInstallRefuseEveryFileActionEntry() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let original = fixture.config(pending: true)
        fixture.save(original)
        fixture.saveInstallRequest(original)
        let library = fixture.library()
        let session = library.windowsInstallSession(for: original)
        let accepted = session.plan
        var current = original
        current.installPending = false // The latest config is otherwise eligible for the clone request.
        fixture.save(current)
        let before = try fixture.snapshot()
        let acknowledgment = try XCTUnwrap(session.start())
        library.reload()
        fixture.cacheFake(in: library, config: current)
        XCTAssertEqual(session.stage, .validating)
        fixture.assertRefused(config: current, library: library)
        await acknowledgment.value

        XCTAssertEqual(session.stage, .preparingSource)
        XCTAssertEqual(fixture.installJobCount, 1)
        fixture.assertRefused(config: current, library: library)
        XCTAssertEqual(fixture.queue.count, 0)
        XCTAssertEqual(session.plan, accepted)
        XCTAssertTrue(library.windowsInstallSession(for: current) === session)
        XCTAssertEqual(try fixture.snapshot(), before)
    }

    func testBusyAndAcceptedGenericLifecycleRefuseRequestsAndFinalActions() async throws {
        do {
            let fixture = Fixture()
            defer { fixture.clean() }
            let config = fixture.config()
            fixture.save(config)
            let before = try fixture.snapshot()
            let library = fixture.library()
            let model = fixture.cacheFake(in: library, config: config)
            model.busy = true
            XCTAssertTrue(model.hasAcceptedOperation)
            fixture.assertRefused(config: config, library: library)
            XCTAssertEqual(fixture.queue.count, 0)
            XCTAssertEqual(try fixture.snapshot(), before)
        }
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.config()
        fixture.save(config)
        let before = try fixture.snapshot()
        let library = fixture.library()
        let model = fixture.cacheFake(in: library, config: config)
        let acknowledged = await fixture.acknowledgeFakeStart(model) {
            XCTAssertTrue(model.lifecycleBusy)
            XCTAssertTrue(model.hasAcceptedOperation)
            fixture.assertRefused(config: config, library: library)
        }
        guard acknowledged else { return }

        XCTAssertFalse(model.lifecycleBusy)
        XCTAssertFalse(model.busy)
        XCTAssertTrue(model.running) // Optimistic observation; the accepted-start deadline still owns work.
        XCTAssertTrue(model.hasAcceptedOperation)
        XCTAssertEqual(fixture.backend(for: config).calls.starts, 1)
        XCTAssertEqual(fixture.backend(for: config).calls.completedStarts, 1)
        fixture.assertRefused(config: config, library: library)
        XCTAssertEqual(fixture.queue.count, 0)
        XCTAssertEqual(try fixture.snapshot(), before)
    }

    func testObservedRunningAloneDoesNotOwnAnAcceptedOperation() throws {
        for action in Entry.finalActions {
            let fixture = Fixture()
            defer { fixture.clean() }
            let config = fixture.config()
            fixture.save(config)
            let before = try fixture.snapshot()
            let library = fixture.library()
            let model = fixture.cacheFake(in: library, config: config)
            model.running = true
            XCTAssertFalse(model.lifecycleBusy)
            XCTAssertFalse(model.hasAcceptedOperation)

            fixture.assertAccepted(action, config: config, library: library)

            XCTAssertEqual(try fixture.snapshot(), before)
        }
    }

    func testChangedAndRemovedRegistrationsRefuseCapturedConfigurations() throws {
        for removed in [false, true] {
            let fixture = Fixture()
            defer { fixture.clean() }
            let original = fixture.config()
            fixture.save(original)
            let library = fixture.library()
            fixture.cacheFake(in: library, config: original)
            var current = original
            current.memMiB = 8192
            if removed {
                try FileManager.default.removeItem(at: fixture.registration(original))
            } else {
                fixture.save(current)
            }
            library.reload()
            // reload evicts the old model; re-cache a fake before the old baseline can use a fallback backend.
            fixture.cacheFake(in: library, config: removed ? original : current)
            let before = try fixture.snapshot()

            fixture.assertRefused(config: original, library: library)

            XCTAssertEqual(fixture.queue.count, 0)
            XCTAssertEqual(try fixture.snapshot(), before)
        }
    }

    func testFinalActionsRecheckAFormerlyValidRequest() throws {
        for final in [Entry.deletion, .clone] {
            for changedRegistration in [false, true] {
                let fixture = Fixture()
                defer { fixture.clean() }
                let original = fixture.config()
                fixture.save(original)
                let library = fixture.library()
                fixture.cacheFake(in: library, config: original)
                let request: Entry = final == .deletion ? .requestDeletion : .requestClone
                request.invoke(original, library: library, destination: fixture.destination)
                XCTAssertEqual(final == .deletion ? library.pendingDeletion : library.pendingWindowsClone, original)
                XCTAssertNil(request.error(in: library))
                if changedRegistration {
                    var changed = original
                    changed.displayName = "Changed after confirmation opened"
                    fixture.save(changed)
                    library.reload()
                    fixture.cacheFake(in: library, config: changed)
                } else {
                    let session = try XCTUnwrap(library.hvfRuntimeSession(for: original))
                    session.connectionState = .booting
                }
                let before = try fixture.snapshot()

                fixture.assertRefused([final], config: original, library: library)

                XCTAssertNil(final == .deletion ? library.pendingDeletion : library.pendingWindowsClone)
                XCTAssertEqual(fixture.queue.count, 0)
                XCTAssertEqual(try fixture.snapshot(), before)
            }
        }
    }

    func testReservationsRemainIndependentAcrossSlugsAndLibraryRoots() throws {
        for action in Entry.finalActions {
            let fixture = Fixture()
            defer { fixture.clean() }
            let first = fixture.config("first"), second = fixture.config("second")
            fixture.save(first); fixture.save(second)
            let before = try fixture.snapshot()
            let library = fixture.library()
            fixture.cacheFake(in: library, config: first)
            fixture.cacheFake(in: library, config: second)
            fixture.assertAccepted(action, config: first, library: library)
            fixture.assertAccepted(action, config: second, library: library)
            XCTAssertEqual(fixture.queue.count, 2)
            XCTAssertEqual(try fixture.snapshot(), before)

            let other = Fixture()
            defer { other.clean() }
            let sameSlug = other.config("first")
            other.save(sameSlug)
            let otherBefore = try other.snapshot()
            let otherLibrary = other.library()
            other.cacheFake(in: otherLibrary, config: sameSlug)
            fixture.assertRefused([action], config: first, library: library)
            other.assertAccepted(action, config: sameSlug, library: otherLibrary)
            XCTAssertEqual(other.queue.count, 1)
            XCTAssertEqual(try other.snapshot(), otherBefore)
            XCTAssertEqual(try fixture.snapshot(), before)
        }
    }
}
