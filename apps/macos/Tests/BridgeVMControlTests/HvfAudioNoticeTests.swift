import XCTest
@testable import BridgeVMControl

/// The VMM's audio failure lines (as printed to the run's log) reach the user as one sentence.
@MainActor
final class HvfAudioNoticeTests: XCTestCase {
    func testTheVMMsAudioFailureLinesBecomeANotice() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root) { _ in false }
        let lines = [
            "HDA CoreAudio output: s16le 48000 Hz stereo, enabled",
            "hda CoreAudio stats: frames_rendered=0 drops=0",
        ]
        for event in BvAgentEvent.parse(lines: lines) { session.handle(event) }
        XCTAssertNil(session.audioNotice.text)
        let mic = "HDA capture: the Mac microphone did not start (AudioQueueNewInput failed with OSStatus -50 (0xffffffce)); the guest microphone records silence"
        for event in BvAgentEvent.parse(lines: [mic]) { session.handle(event) }
        XCTAssertEqual(session.audioNotice.text, "The Mac microphone didn’t start, so Windows recorded silence. Windows tries again the next time an app starts recording.")
        // The next recording opened it: the notice goes (hda_microphone.rs supervise).
        for event in BvAgentEvent.parse(lines: ["HDA capture: the Mac microphone is on again"]) { session.handle(event) }
        XCTAssertNil(session.audioNotice.text)
        for event in BvAgentEvent.parse(lines: [mic]) { session.handle(event) }
        let output = "HDA CoreAudio output: off for this run, the Mac's audio output did not start (AudioQueueNewOutput failed with OSStatus -50 (0xffffffce)); Windows runs without sound, restart the VM to try again"
        for event in BvAgentEvent.parse(lines: [output]) { session.handle(event) }
        XCTAssertEqual(session.audioNotice.text, "Windows has no sound this time because the Mac’s audio output didn’t start. Restart the VM to try again.")
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertNil(session.audioNotice.text, "a new run starts afresh")
    }
}
