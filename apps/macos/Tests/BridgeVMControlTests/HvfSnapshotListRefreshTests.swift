import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfSnapshotListRefreshTests: XCTestCase {
    func testIdleGuidanceTracksRuntimeStateAndKeepsOperationFeedback() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let card = HvfWindowsSnapshotCard(config: session.config, session: session)

        XCTAssertEqual(card.statusText, "Create a snapshot to save the Windows disk and boot settings.")
        session.connectionState = .booting
        XCTAssertEqual(card.statusText, "To create or restore a snapshot, shut down Windows and wait for other VM operations to finish.")
        session.connectionState = .stopped

        let work = try XCTUnwrap(session.snapshotModel.create { _ in nil })
        XCTAssertEqual(card.statusText, "Creating snapshot…")
        await work.value
        XCTAssertEqual(card.statusText, "Snapshot created")
        session.connectionState = .booting
        XCTAssertEqual(card.statusText, "To create or restore a snapshot, shut down Windows and wait for other VM operations to finish.",
                       "a start ends the last result")
        session.connectionState = .stopped
        await session.snapshotModel.create { _ in throw HvfWindowsSnapshotCommand.notStopped() }?.value
        XCTAssertEqual(card.statusText, "Shut down the VM and wait for other operations to finish before creating a snapshot.")
    }

    func testReopenedCardRefreshesWhenAnExistingSnapshotOperationFinishes() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let (release, continuation) = AsyncStream<Void>.makeStream()
        defer { continuation.finish() }
        let work = try XCTUnwrap(session.snapshotModel.create { _ in
            for await _ in release { break }
            return nil
        })

        let probe = ReadProbe()
        var reopened = HvfWindowsSnapshotCard(config: session.config, session: session)
        reopened.listEntries = { probe.read($0) }
        let host = NSHostingView(rootView: reopened.environment(\.controlActiveState, .inactive))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 680, height: 360),
                              styleMask: [.titled], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        try await waitForReads(1, probe: probe, host: host)
        continuation.finish()
        await work.value
        try await waitForReads(2, probe: probe, host: host)
    }

    func testReturningToSettingsLoadsExternalChangesWithoutReadingForSessionEvents() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let bundle = try XCTUnwrap(HvfDiskGrow.bundle(config: session.config))
        let snapshot = HvfWindowsSnapshotStore.directory(bundle: bundle).appendingPathComponent("latest.snapshot")
        try FileManager.default.createDirectory(at: snapshot, withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: snapshot.appendingPathComponent("manifest.json"))
        let probe = ReadProbe()
        var card = HvfWindowsSnapshotCard(config: session.config, session: session)
        card.listEntries = { probe.read($0) }
        let host = NSHostingView(rootView: card.environment(\.controlActiveState, .inactive))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 680, height: 360),
                              styleMask: [.titled], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        try await waitForReads(1, probe: probe, host: host)
        XCTAssertEqual(probe.names, [["latest.snapshot"]])

        session.append(.unknown("fixture guest status"))
        await FirstRunOptionalRecoveryFixture.settle(host)
        XCTAssertEqual(probe.names.count, 1)

        try "External checkpoint".write(to: HvfWindowsSnapshotStore.nameFile(snapshot),
                                          atomically: true, encoding: .utf8)
        host.rootView = card.environment(\.controlActiveState, .key)
        try await waitForReads(2, probe: probe, host: host)
        XCTAssertEqual(probe.names, [["latest.snapshot"], ["External checkpoint"]])
    }

    private func waitForReads(_ count: Int, probe: ReadProbe, host: NSView) async throws {
        let deadline = ContinuousClock.now + .seconds(2)
        while probe.names.count < count, ContinuousClock.now < deadline {
            host.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(probe.names.count, count)
    }
}

private final class ReadProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var observed: [[String]] = []
    var names: [[String]] { lock.withLock { observed } }

    func read(_ bundle: URL) -> [HvfWindowsSnapshotStore.Entry] {
        XCTAssertFalse(Thread.isMainThread)
        let entries = HvfWindowsSnapshotStore.list(bundle: bundle)
        lock.withLock { observed.append(entries.map { $0.name ?? $0.id }) }
        return entries
    }
}
