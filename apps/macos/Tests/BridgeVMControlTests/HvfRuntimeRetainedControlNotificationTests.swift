import XCTest
import Combine
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeRetainedControlNotificationTests: XCTestCase {
    func testRealIndexNotifiesInsertionAndDismissalWithoutDuplicateSubscriptions() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config()
        f.work.save(config)
        let library = f.work.library()
        let session = try f.work.runtime(config, in: library)
        let descriptor = LibraryRetainedControlDescriptor.runtime(config: config, session: session)
        let store = LibraryRetainedControlStore()
        var notifications = 0
        store.onChange = { notifications += 1 }
        let before = f.effects, bytes = try f.work.snapshot()

        store.capture([descriptor]) // An inactive, never-captured handle does not become a record.
        XCTAssertTrue(store.records.isEmpty)
        XCTAssertEqual(notifications, 0)
        session.connectionState = .booting
        store.capture([descriptor, descriptor])
        XCTAssertEqual(store.records.count, 1)
        XCTAssertEqual(notifications, 1, "Insertion notifies once, without initial publisher replay")
        let token = try XCTUnwrap(store.records.first?.id)
        store.capture([descriptor])
        XCTAssertEqual(store.records.map(\.id), [token])
        XCTAssertEqual(notifications, 1)
        XCTAssertFalse(store.dismiss("unknown retained token"))
        XCTAssertFalse(store.dismiss(token))
        XCTAssertEqual(notifications, 1)

        session.connectionState = .timedOut
        XCTAssertEqual(notifications, 2, "Repeated capture must not duplicate state subscriptions")
        session.events = [.unknown("leaf-only event")]
        session.lastHeartbeatAge = 12
        XCTAssertEqual(notifications, 2)
        session.connectionState = .stopped
        XCTAssertEqual(notifications, 3)
        XCTAssertEqual(store.records.map(\.id), [token], "Terminal state retains the exact record")
        XCTAssertTrue(store.dismiss(token))
        XCTAssertEqual(notifications, 4)
        XCTAssertTrue(store.records.isEmpty)
        XCTAssertFalse(store.dismiss(token))
        session.connectionState = .booting // Held here after dismissal, with no process or attachment.
        XCTAssertEqual(notifications, 4, "Dismissal must cancel this record's state subscription")
        XCTAssertEqual(f.effects, before)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }

    func testLibraryRelaysOnlyRuntimeStateWhileWillSetPreservesSelectionAndMembership() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config()
        f.work.save(config)
        let library = f.work.library()
        let session = try f.work.runtime(config, in: library)
        session.connectionState = .booting
        library.selectedID = config.slug
        try f.removeRegistration(config)
        library.reload()
        let token = try XCTUnwrap(f.runtimeRecord(session, in: library)?.id)
        XCTAssertEqual(library.selectedID, token)
        let before = f.effects, bytes = try f.work.snapshot()
        var observedStates: [HvfConnectionState] = []
        let subscription = library.objectWillChange.sink { [weak library, weak session] in
            guard let library, let session else { return XCTFail("Observed owners must still exist") }
            observedStates.append(session.connectionState)
            XCTAssertEqual(library.retainedControlRecords.map(\.id), [token])
            XCTAssertEqual(library.selectedID, token)
            XCTAssertEqual(library.selectedRetainedControl?.id, token)
        }
        defer { subscription.cancel() }

        session.events = [.unknown("retained runtime log")]
        session.lastHeartbeatAge = 7
        XCTAssertTrue(observedStates.isEmpty, "Leaf data must not invalidate the entire library")
        session.connectionState = .connected(host: "fixture")
        XCTAssertEqual(observedStates, [.booting], "Published emits before storing the new value")
        XCTAssertEqual(session.connectionState, .connected(host: "fixture"))
        session.connectionState = .stopped
        XCTAssertEqual(observedStates, [.booting, .connected(host: "fixture")])
        XCTAssertEqual(library.selectedID, token)
        XCTAssertEqual(library.retainedControlRecords.map(\.id), [token])
        XCTAssertFalse(try XCTUnwrap(library.selectedRetainedControl).descriptor.isActive)
        XCTAssertEqual(f.assertRuntimeRoute(session, config: config, in: library), token)
        XCTAssertEqual(f.effects, before)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }

    func testInstallerCancelLogStaysLocalAndAcknowledgedFailureRelaysWithoutReload() async throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config(pending: true)
        f.work.save(config)
        f.work.saveRequest(config)
        let library = f.work.library()
        library.selectedID = config.slug
        var token: String?
        var observedStages: [HvfWindowsInstallStage] = []
        var subscription: AnyCancellable?
        var capturedEffects: [Int] = []
        var capturedBytes: [String: Data] = [:]
        defer { subscription?.cancel() }

        let session = try await f.withValidatingInstall(config, library: library) { session in
            XCTAssertEqual(session.stage, .validating)
            try f.removeRegistration(config)
            library.reload()
            let capturedToken = try XCTUnwrap(f.installRecord(session, in: library)?.id)
            token = capturedToken
            XCTAssertEqual(library.selectedID, capturedToken)
            capturedEffects = f.effects
            capturedBytes = try f.work.snapshot()
            subscription = library.objectWillChange.sink { [weak library, weak session] in
                guard let library, let session else { return XCTFail("Observed owners must still exist") }
                observedStages.append(session.stage)
                XCTAssertEqual(library.retainedControlRecords.map(\.id), [capturedToken])
                XCTAssertEqual(library.selectedID, capturedToken)
            }
            session.cancel()
            XCTAssertTrue(session.logLines.contains("사용자가 설치를 취소했습니다."))
            XCTAssertEqual(session.stage, .validating)
            XCTAssertTrue(observedStages.isEmpty, "Cancel's log alone must not relay to the parent")
            XCTAssertEqual(f.effects, capturedEffects)
        }
        // The helper has released the gate and awaited its accepted validation handle.
        XCTAssertEqual(session.stage, .failed("설치가 취소되었습니다."))
        XCTAssertEqual(observedStages, [.validating], "The stage relay must preserve willSet membership")
        let selectedToken = try XCTUnwrap(token)
        XCTAssertEqual(library.selectedID, selectedToken)
        XCTAssertEqual(f.assertInstallRoute(session, config: config, in: library), selectedToken)
        XCTAssertTrue(session.logLines.contains("사용자가 설치를 취소했습니다."))
        XCTAssertEqual(f.work.installJobCount, 0)
        XCTAssertFalse(f.work.validator.snapshot.gateTimedOut)
        XCTAssertEqual(f.effects, capturedEffects)
        XCTAssertEqual(try f.work.snapshot(), capturedBytes)
    }
}
