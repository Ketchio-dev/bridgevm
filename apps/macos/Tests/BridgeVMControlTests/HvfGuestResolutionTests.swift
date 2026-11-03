import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfGuestResolutionTests: XCTestCase {
    private func ownedArguments(_ config: HvfEngineConfig) -> [String] {
        config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p", ownedRuntime: true)
    }

    private func passes(_ resolution: String, in arguments: [String]) -> Bool {
        zip(arguments, arguments.dropFirst()).contains { $0 == ("--guest-resolution", resolution) }
    }

    /// Automatic is the default; only the user's saved choice launches with an explicit resolution.
    func testGuestResolutionIsAutomaticUntilTheUserSavesOne() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertNil(session.config.guestResolution, "An absent key leaves the resolution to the VMM")
        XCTAssertFalse(ownedArguments(session.config).contains("--guest-resolution"))
        XCTAssertFalse(session.config.wrapperArguments().contains("--guest-resolution"))

        XCTAssertTrue(session.applyGuestResolution("1920x1080") == ("1920x1080", nil))
        let registration = try Data(contentsOf: fixture.registration(saved))
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: registration).guestResolution, "1920x1080")
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.guestResolution, "1920x1080")
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        XCTAssertTrue(passes("1920x1080", in: ownedArguments(launch)))
        XCTAssertTrue(passes("1920x1080", in: launch.wrapperArguments()))
        XCTAssertEqual(session.config, launch, "Rebound in place with the saved choice, as a native CLI start requires")

        // Automatic again restores the never-touched registration.
        XCTAssertTrue(session.applyGuestResolution(nil) == (nil, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).guestResolution)
        let restored = try Data(contentsOf: fixture.registration(saved))
        XCTAssertFalse(String(decoding: restored, as: UTF8.self).contains("guestResolution"))
    }
}
