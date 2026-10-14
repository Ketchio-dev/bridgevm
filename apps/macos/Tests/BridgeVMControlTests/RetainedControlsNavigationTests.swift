import XCTest
@testable import BridgeVMControl

@MainActor
final class RetainedControlsNavigationTests: XCTestCase {
    func testCurrentSidebarKeepsCapturedRuntimeReachableAfterSelectingRegisteredVM() throws {
        try assertSidebarRoute(selectedBeforeCapture: "lost")
    }

    func testCurrentSidebarExposesRuntimeCapturedWhileAnotherVMIsSelected() throws {
        try assertSidebarRoute(selectedBeforeCapture: "other")
    }

    func testCapturedInstallReentryKeepsCancellationAndTerminalDismissal() async throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let lost = f.work.config("install", pending: true), other = f.work.config("other")
        f.work.save(lost); f.work.saveRequest(lost); f.work.save(other)
        let library = f.work.library()
        library.selectedID = other.slug
        let session = try await f.withValidatingInstall(lost, library: library) { session in
            try f.removeRegistration(lost)
            library.reload()
            XCTAssertEqual(library.selectedID, other.slug)
            let record = try XCTUnwrap(f.installRecord(session, in: library))
            let navigation = try currentNavigation(library, fixture: f)
            let before = f.effects, bytes = try f.work.snapshot()
            navigation.selection.wrappedValue = record.id
            XCTAssertEqual(f.assertInstallRoute(session, config: lost, in: library), record.id)
            XCTAssertFalse(library.dismissRetainedControl(record.id), "Active work remains retained")
            session.cancel()
            XCTAssertEqual(session.stage, .cancelling)
            XCTAssertEqual(f.assertInstallRoute(session, config: lost, in: library), record.id)
            XCTAssertEqual(f.effects, before)
            XCTAssertEqual(try f.work.snapshot(), bytes)
        }
        XCTAssertEqual(session.stage, .cancelled)
        let detail = try XCTUnwrap(f.selectedView(in: library))
        let terminal = try XCTUnwrap(f.values(RetainedInstallControlView.self, in: detail.body).first)
        XCTAssertTrue(terminal.session === session)
        terminal.dismiss()
        XCTAssertTrue(library.retainedControlRecords.isEmpty)
        XCTAssertEqual(library.selectedID, other.slug)
        XCTAssertEqual(f.work.installJobCount, 0)
    }

    func testEmptyRetainedNavigationCannotChangeOrdinarySelection() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let config = f.work.config("ordinary")
        f.work.save(config)
        let library = f.work.library()
        let before = f.effects
        let navigation = try XCTUnwrap(f.values(LibraryRetainedControlsNavigation.self,
            in: LibrarySidebar(library: library).body).first)
        XCTAssertTrue(f.values(RetainedControlsSidebarEntry.self, in: navigation.body).isEmpty)
        navigation.selection.wrappedValue = "unknown-token"
        navigation.selection.wrappedValue = nil
        XCTAssertEqual(library.selectedID, config.slug)
        XCTAssertEqual(f.effects, before)
    }

    private func currentNavigation(_ library: LibraryModel,
        fixture f: HvfRuntimeRetainedControlFixture) throws -> LibraryRetainedControlsNavigation {
        let navigation = try XCTUnwrap(f.values(LibraryRetainedControlsNavigation.self,
            in: LibrarySidebar(library: library).body).first)
        let entries = f.values(RetainedControlsSidebarEntry.self, in: navigation.body)
        XCTAssertEqual(entries.count, 1, "Current sidebar must expose retained controls")
        XCTAssertTrue(entries.first?.library === library)
        return navigation
    }

    private func assertSidebarRoute(selectedBeforeCapture: String) throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let lost = f.work.config("lost"), other = f.work.config("other")
        f.work.save(lost); f.work.save(other)
        let library = f.work.library()
        library.selectedID = selectedBeforeCapture == "lost" ? lost.slug : other.slug
        let session = try f.work.runtime(lost, in: library)
        session.connectionState = .connected(host: "owned-observation")
        try f.removeRegistration(lost)
        library.reload()
        if selectedBeforeCapture == "other" { XCTAssertEqual(library.selectedID, other.slug) }
        let record = try XCTUnwrap(f.runtimeRecord(session, in: library))
        library.selectedID = other.slug // Existing workspace row selection, before re-entry.
        let before = f.effects, bytes = try f.work.snapshot()
        let navigation = try currentNavigation(library, fixture: f)
        XCTAssertEqual(library.selectedID, other.slug)
        library.proMode = true
        navigation.selection.wrappedValue = record.id
        XCTAssertFalse(library.proMode)
        XCTAssertEqual(f.assertRuntimeRoute(session, config: lost, in: library), record.id)
        XCTAssertEqual(session.connectionState, .connected(host: "owned-observation"))
        session.connectionState = .stopped
        let detail = try XCTUnwrap(f.selectedView(in: library))
        let terminal = try XCTUnwrap(f.values(RetainedRuntimeControlView.self, in: detail.body).first)
        XCTAssertTrue(terminal.session === session)
        terminal.dismiss()
        XCTAssertTrue(library.retainedControlRecords.isEmpty)
        XCTAssertEqual(library.selectedID, other.slug)
        navigation.selection.wrappedValue = record.id // Stale list selection cannot revive a dismissed handle.
        XCTAssertEqual(library.selectedID, other.slug)
        XCTAssertTrue(f.values(RetainedControlsSidebarEntry.self, in: navigation.body).isEmpty)
        XCTAssertEqual(f.effects, before)
        XCTAssertEqual(try f.work.snapshot(), bytes)
    }
}
