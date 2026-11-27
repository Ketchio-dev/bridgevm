import XCTest
@testable import BridgeVMControl

/// A microphone permission that never reaches macOS: no test may show its prompt.
private final class FakeMicrophonePermission: HvfMicrophonePermission {
    var status: HvfMicrophoneStatus
    private(set) var requests: [@MainActor (Bool) -> Void] = []
    init(_ status: HvfMicrophoneStatus) { self.status = status }
    func requestAccess(_ completion: @escaping @MainActor (Bool) -> Void) { requests.append(completion) }
}

@MainActor
final class HvfMicrophoneTests: XCTestCase {
    private func ownedArguments(_ config: HvfEngineConfig, _ permission: FakeMicrophonePermission) -> [String] {
        config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p",
                               ownedRuntime: true, microphone: permission)
    }

    /// Off until the user turns it on; on, the VM gets the microphone only with macOS's permission.
    func testTheMicrophoneIsOffUntilTheUserTurnsItOnAndReachesTheVMOnlyWithPermission() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let allowed = FakeMicrophonePermission(.authorized), refused = FakeMicrophonePermission(.denied)
        XCTAssertFalse(session.config.microphoneEnabled, "An absent key keeps the microphone off")
        XCTAssertFalse(ownedArguments(session.config, allowed).contains("--helper-hda-microphone"))
        XCTAssertFalse(session.config.wrapperArguments(microphone: allowed).contains("--hda-microphone"))

        XCTAssertTrue(session.applyMicrophoneToggle(true, permission: allowed) { _ in XCTFail("no notice") } == (true, nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.microphoneEnabled, true)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        XCTAssertTrue(launch.microphoneEnabled)
        XCTAssertTrue(ownedArguments(launch, allowed).contains("--helper-hda-microphone"))
        XCTAssertTrue(launch.wrapperArguments(microphone: allowed).contains("--hda-microphone"))
        // Without macOS's permission the VM still starts, with sound, and Windows records silence.
        XCTAssertFalse(ownedArguments(launch, refused).contains("--helper-hda-microphone"))
        XCTAssertTrue(ownedArguments(launch, refused).contains("--helper-hda"))
        XCTAssertFalse(launch.wrapperArguments(microphone: refused).contains("--hda-microphone"))
        XCTAssertEqual(HvfMicrophoneNotice.text(.denied), HvfMicrophoneNotice.denied)
        XCTAssertNil(HvfMicrophoneNotice.text(.authorized))

        // Turning it off again restores the never-touched registration.
        XCTAssertTrue(session.applyMicrophoneToggle(false, permission: allowed) { _ in } == (false, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).microphoneEnabled)
        XCTAssertTrue(allowed.requests.isEmpty && refused.requests.isEmpty, "a decided permission is never asked again")
    }

    /// macOS asks only after the user turns the setting on; a refusal shows the notice
    /// and the setting stays on, so a later permission in System Settings takes effect.
    func testMacOSAsksOnlyAfterTheUserTurnsItOnAndARefusalShowsTheNotice() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let undecided = FakeMicrophonePermission(.undetermined)
        var notices: [String] = []
        XCTAssertTrue(session.applyMicrophoneToggle(false, permission: undecided) { notices.append($0) } == (false, nil))
        XCTAssertTrue(undecided.requests.isEmpty, "turning it off never asks")

        XCTAssertTrue(session.applyMicrophoneToggle(true, permission: undecided) { notices.append($0) } == (true, nil))
        XCTAssertEqual(undecided.requests.count, 1)
        undecided.requests[0](false)
        XCTAssertEqual(notices, [HvfMicrophoneNotice.denied])
        XCTAssertEqual(library.latestConfiguration(for: saved).microphoneEnabled, true)

        // Already refused: no prompt, the notice at once.
        let refused = FakeMicrophonePermission(.denied)
        XCTAssertTrue(session.applyMicrophoneToggle(true, permission: refused) { notices.append($0) }
            == (true, HvfMicrophoneNotice.denied))
        XCTAssertTrue(refused.requests.isEmpty)
    }

    /// macOS forgets its answer after a reset, a re-signed build or on another Mac. A start with
    /// the setting on then asks first and launches after any answer; a decided permission starts at once.
    func testAStartWithTheSettingOnAsksMacOSFirstWhenItHasNotAskedYet() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.microphoneEnabled = true; fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(library.latestConfiguration(for: saved),
            rootURL: library.rootURL, e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        XCTAssertTrue(launch.microphoneEnabled && launch.audioEnabled)
        // Stands in for the launch: the ticket finishes with this only once the worker has run.
        session.guiStartWorker = { _ in .init(result: .refused("fixture: launched")) }
        func start(_ permission: FakeMicrophonePermission) throws -> HvfGUIStartOperation {
            guard case let .accepted(ticket) = session.requestGUIStart(configuration: launch, microphone: permission) else {
                throw CocoaError(.executableRuntimeMismatch)
            }
            return ticket
        }
        func launched(_ ticket: HvfGUIStartOperation) async throws {
            let deadline = ProcessInfo.processInfo.systemUptime + 2
            while ticket.workerPending, ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
            XCTAssertEqual(ticket.outcome, .refused("fixture: launched"))
        }

        let undecided = FakeMicrophonePermission(.undetermined)
        let first = try start(undecided)
        XCTAssertEqual(undecided.requests.count, 1)
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertTrue(first.workerPending, "the launch waits for macOS's answer")
        try XCTUnwrap(undecided.requests.first)(false)
        try await launched(first)
        XCTAssertEqual(undecided.requests.count, 1)

        for status in [HvfMicrophoneStatus.authorized, .denied] {
            let decided = FakeMicrophonePermission(status)
            try await launched(try start(decided))
            XCTAssertTrue(decided.requests.isEmpty, "\(status) is never asked again")
        }
    }
}
