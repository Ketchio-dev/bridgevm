import XCTest
import Combine
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeRetainedControlLifecycleTests: XCTestCase {
    func testTerminalRecordOwnsSessionAfterOrdinaryStorePruning() async throws {
        do {
            let f = try HvfRuntimeRetainedControlFixture()
            defer { f.clean() }
            let config = f.work.config()
            f.work.save(config)
            let library = f.work.library()
            var session: HvfEngineSession? = try f.work.runtime(config, in: library)
            weak var observed = session
            session?.connectionState = .booting
            session?.events = [.unknown("terminal history remains")]
            library.selectedID = config.slug
            try f.removeRegistration(config); library.reload()
            let token = try XCTUnwrap(f.runtimeRecord(try XCTUnwrap(session), in: library)?.id)
            session = nil
            observed?.connectionState = .stopped
            library.reload()
            XCTAssertNotNil(observed, "The retained index must own the terminal session after ordinary pruning")
            XCTAssertEqual(library.selectedID, token)
            XCTAssertEqual(f.assertRuntimeRoute(try XCTUnwrap(observed), config: config, in: library), token)
            XCTAssertEqual(observed?.events, [.unknown("terminal history remains")])
            XCTAssertEqual(library.retainedControlRecords.count, 1)
        }
        do {
            let f = try HvfRuntimeRetainedControlFixture()
            defer { f.clean() }
            let config = f.work.config(pending: true)
            f.work.save(config); f.work.saveRequest(config)
            let library = f.work.library()
            library.selectedID = config.slug
            var token: String?
            var session: HvfWindowsInstallSession? = try await f.withValidatingInstall(config, library: library) { session in
                try f.removeRegistration(config); library.reload()
                token = f.installRecord(session, in: library)?.id
                session.cancel()
            }
            weak var observed = session
            session = nil // The helper has also released its completed Task handle.
            library.reload()
            XCTAssertNotNil(observed)
            XCTAssertNotNil(token)
            XCTAssertEqual(library.selectedID, token)
            XCTAssertEqual(f.assertInstallRoute(try XCTUnwrap(observed), config: config, in: library), token)
            XCTAssertEqual(observed?.stage, .failed("설치가 취소되었습니다."))
            XCTAssertTrue(observed?.logLines.contains("사용자가 설치를 취소했습니다.") == true)
        }
    }

    func testOnlyFinishedRecordsDismissWithPreciseSelectionFallback() async throws {
        for kind in ["runtime", "install"] {
            for selectionCase in ["selected-with-row", "selected-without-row", "unselected-with-row"] {
                let f = try HvfRuntimeRetainedControlFixture()
                defer { f.clean() }
                let config = f.work.config(pending: kind == "install"), normal = f.work.config("normal")
                f.work.save(config)
                if kind == "install" { f.work.saveRequest(config) }
                if selectionCase != "selected-without-row" { f.work.save(normal) }
                let library = f.work.library()
                library.selectedID = config.slug
                library.operationError = "unrelated notice"
                var token: String?
                if kind == "runtime" {
                    let session = try f.work.runtime(config, in: library)
                    session.connectionState = .connected(host: "fixture")
                    try f.removeRegistration(config); library.reload()
                    token = try XCTUnwrap(f.runtimeRecord(session, in: library)?.id)
                    let before = f.effects, state = session.connectionState, events = session.events
                    XCTAssertFalse(library.dismissRetainedControl(try XCTUnwrap(token)))
                    XCTAssertEqual(session.connectionState, state)
                    XCTAssertEqual(session.events, events)
                    XCTAssertEqual(f.effects, before)
                    session.connectionState = .stopped
                } else {
                    _ = try await f.withValidatingInstall(config, library: library) { session in
                        try f.removeRegistration(config); library.reload()
                        token = try XCTUnwrap(f.installRecord(session, in: library)?.id)
                        let before = f.effects, logs = session.logLines, started = session.startedAt
                        XCTAssertFalse(library.dismissRetainedControl(try XCTUnwrap(token)))
                        XCTAssertEqual(session.stage, .validating)
                        XCTAssertEqual(session.logLines, logs)
                        XCTAssertEqual(session.startedAt, started)
                        XCTAssertEqual(f.effects, before)
                        session.cancel()
                    }
                }
                let finishedToken = try XCTUnwrap(token)
                library.reload()
                if selectionCase == "unselected-with-row" { library.selectedID = normal.slug }
                let expectedSelection = selectionCase == "unselected-with-row" ? normal.slug : library.vms.first?.slug
                let bytes = try f.work.snapshot(), before = f.effects
                XCTAssertFalse(library.dismissRetainedControl("unknown retained token"))
                XCTAssertTrue(library.dismissRetainedControl(finishedToken))
                XCTAssertEqual(library.selectedID, expectedSelection)
                XCTAssertTrue(library.retainedControlRecords.isEmpty)
                XCTAssertFalse(library.dismissRetainedControl(finishedToken))
                XCTAssertEqual(library.operationError, "unrelated notice")
                XCTAssertEqual(f.effects, before)
                XCTAssertEqual(try f.work.snapshot(), bytes)
            }
        }
    }

    func testSameSlugRecreationAndSecondRemovalKeepBothObjectTokens() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let original = f.work.config()
        f.work.save(original)
        let library = f.work.library()
        library.selectedID = original.slug
        let old = try f.work.runtime(original, in: library)
        old.connectionState = .booting
        try f.removeRegistration(original); library.reload()
        let oldToken = try XCTUnwrap(f.assertRuntimeRoute(old, config: original, in: library))
        var replacementConfig = original
        replacementConfig.bundlePath = f.work.root.appendingPathComponent("replacement/bundle").path
        replacementConfig.displayName = "new registered work"
        replacementConfig.memMiB = 8192
        f.work.save(replacementConfig); library.reload()
        XCTAssertEqual(library.selectedID, oldToken)
        XCTAssertEqual(f.runtimeRecord(old, in: library)?.descriptor.config, original)
        f.assertRuntimeRoute(old, config: original, in: library)
        var notifications = 0
        let subscription = library.objectWillChange.sink { notifications += 1 }
        defer { subscription.cancel() }
        let beforeStateChange = notifications
        old.connectionState = .timedOut
        XCTAssertEqual(notifications, beforeStateChange + 1, "Re-registration preserves the old subscription")

        old.connectionState = .stopped
        library.reload()
        let replacement = try f.work.runtime(replacementConfig, in: library)
        XCTAssertFalse(replacement === old)
        library.selectedID = replacementConfig.slug
        let normalBody = LibraryDetailView(library: library).body
        XCTAssertTrue(try XCTUnwrap(f.values(HvfEngineView.self, in: normalBody).first).session === replacement)
        replacement.connectionState = .booting
        try f.removeRegistration(replacementConfig); library.reload()
        let newToken = try XCTUnwrap(f.runtimeRecord(replacement, in: library)?.id)
        XCTAssertNotEqual(newToken, oldToken)
        XCTAssertEqual(library.retainedControlRecords.count, 2)
        XCTAssertEqual(library.selectedID, newToken, "An old finished matching slug must not win selection")
        f.assertRuntimeRoute(replacement, config: replacementConfig, in: library)
        library.selectedID = oldToken
        f.assertRuntimeRoute(old, config: original, in: library)
        library.selectedID = newToken
        let bytes = try f.work.snapshot(), effects = f.effects
        XCTAssertTrue(library.dismissRetainedControl(oldToken))
        XCTAssertEqual(library.selectedID, newToken)
        let afterDismissal = notifications
        old.connectionState = .booting // Still held by this test, but its dismissed subscription is gone.
        XCTAssertEqual(notifications, afterDismissal)
        XCTAssertEqual(library.retainedControlRecords.map(\.id), [newToken])
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }

    func testLibraryAndDismissalReleaseSubscriptionsWithoutPrematureSessionRelease() throws {
        for releaseLibraryFirst in [false, true] {
            let f = try HvfRuntimeRetainedControlFixture()
            defer { f.clean() }
            let config = f.work.config()
            f.work.save(config)
            var library: LibraryModel? = f.work.library()
            weak var observedLibrary = library
            var session: HvfEngineSession? = try f.work.runtime(config, in: XCTUnwrap(library))
            weak var observedSession = session
            session?.connectionState = .booting
            library?.selectedID = config.slug
            try f.removeRegistration(config); library?.reload()
            let token = try XCTUnwrap(f.runtimeRecord(try XCTUnwrap(session), in: XCTUnwrap(library))?.id)
            var view = f.selectedView(in: try XCTUnwrap(library))
            XCTAssertNotNil(view)
            view = nil
            XCTAssertNotNil(observedSession)
            var notifications = 0
            let subscription = try XCTUnwrap(library).objectWillChange.sink { notifications += 1 }
            defer { subscription.cancel() }
            if releaseLibraryFirst {
                library = nil
                XCTAssertNil(observedLibrary)
                let before = notifications
                session?.connectionState = .timedOut
                XCTAssertEqual(notifications, before)
                session = nil
                XCTAssertNil(observedSession)
            } else {
                session = nil
                observedSession?.connectionState = .stopped
                library?.reload()
                XCTAssertNotNil(observedSession)
                XCTAssertTrue(library?.dismissRetainedControl(token) == true)
                XCTAssertNil(observedSession)
                library = nil
                XCTAssertNil(observedLibrary)
            }
        }
    }
}
