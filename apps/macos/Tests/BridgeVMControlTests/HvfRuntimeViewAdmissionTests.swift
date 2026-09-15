import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeViewAdmissionTests: XCTestCase {
    private let nonStoppedStates: [HvfConnectionState] = [
        .booting, .connected(host: "TEST-HOST"), .stopping, .timedOut
    ]
    private let observedEvents: [BvAgentEvent] = [
        .serviceStart(tMs: 10), .aliveHeartbeat(tMs: 20), .unknown("retained observation")
    ]

    private func config(_ name: String) -> HvfEngineConfig {
        // Paths are values only: these tests never create media or launch a VM.
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("runtime-admission-" + UUID().uuidString)
        let vm = VMConfig(id: name, name: name, displayName: name, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: name, displayWidth: 1280, displayHeight: 720)
        return HvfEngineConfig(
            targetDiskPath: root.appendingPathComponent("disk.raw").path,
            uefiVarsPath: root.appendingPathComponent("vars.fd").path,
            evidenceDir: root.appendingPathComponent("evidence").path,
            watchdogMs: 9000, ramMiB: 8192, smpCpus: 6, clipboardSync: false,
            shareHostDir: root.appendingPathComponent("share").path,
            shareGuestDir: "C:\\shared", virtioNet: false, audioEnabled: false,
            virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("control.ctl").path,
            performanceRisk: .balanced, vtpmStateDir: root.appendingPathComponent("vtpm").path,
            swtpmBin: root.appendingPathComponent("swtpm").path, vtpmKeyID: name,
            allowsExperimental3D: false, libraryContext: HvfLibraryLaunchContext(config: vm, rootURL: root))
    }

    func testNonStoppedStatesRejectEditedConfigurationWithoutChangingSession() {
        let original = config("original")
        let edited = config("edited")
        for state in nonStoppedStates {
            var probes = 0
            let session = HvfEngineSession(config: original, repoRoot: URL(fileURLWithPath: "/unused")) { _ in
                probes += 1
                return false
            }
            session.connectionState = state
            session.events = observedEvents
            session.lastHeartbeatAge = 12

            XCTAssertFalse(session.acceptStartConfiguration(edited), "\(state)")

            XCTAssertEqual(session.config, original, "\(state)")
            XCTAssertEqual(session.connectionState, state)
            XCTAssertEqual(session.events, observedEvents)
            XCTAssertEqual(session.lastHeartbeatAge, 12)
            XCTAssertEqual(probes, 0)
        }
    }

    func testNonStoppedAppearanceDoesNotProbeOrResetObservedState() {
        let original = config("original")
        for state in nonStoppedStates {
            var probes = 0
            let session = HvfEngineSession(config: original, repoRoot: URL(fileURLWithPath: "/unused")) { _ in
                probes += 1
                return false
            }
            session.connectionState = state
            session.events = observedEvents
            session.lastHeartbeatAge = 12

            XCTAssertFalse(session.attachIfStopped(), "\(state)")

            XCTAssertEqual(probes, 0, "\(state)")
            XCTAssertEqual(session.config, original)
            XCTAssertEqual(session.connectionState, state)
            XCTAssertEqual(session.events, observedEvents)
            XCTAssertEqual(session.lastHeartbeatAge, 12)
        }
    }

    func testStoppedConfigurationAdmissionCopiesExactConfigurationWithoutLaunching() {
        let edited = config("edited")
        var probes = 0
        let session = HvfEngineSession(config: config("original"), repoRoot: URL(fileURLWithPath: "/unused")) { _ in
            probes += 1
            return false
        }
        session.events = observedEvents
        session.lastHeartbeatAge = 12

        XCTAssertTrue(session.acceptStartConfiguration(edited))

        XCTAssertEqual(session.config, edited)
        XCTAssertEqual(session.config.libraryContext?.rootURL, edited.libraryContext?.rootURL)
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertEqual(session.events, observedEvents)
        XCTAssertEqual(session.lastHeartbeatAge, 12)
        XCTAssertEqual(probes, 0)
    }

    func testStoppedAppearanceUsesExistingAttachmentProbeExactlyOnce() {
        let original = config("original")
        var probedPaths: [String] = []
        let session = HvfEngineSession(config: original, repoRoot: URL(fileURLWithPath: "/unused")) { path in
            probedPaths.append(path)
            return false
        }
        session.events = observedEvents
        session.lastHeartbeatAge = 12

        XCTAssertFalse(session.attachIfStopped())

        XCTAssertEqual(probedPaths, [original.targetDiskPath])
        XCTAssertEqual(session.config, original)
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertEqual(session.events, observedEvents)
        XCTAssertEqual(session.lastHeartbeatAge, 12)
    }
}
