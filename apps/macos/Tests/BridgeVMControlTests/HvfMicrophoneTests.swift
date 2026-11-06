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
}
