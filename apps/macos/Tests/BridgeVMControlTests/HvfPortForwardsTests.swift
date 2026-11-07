import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfPortForwardsTests: XCTestCase {
    private func forwardArguments(_ config: HvfEngineConfig) -> [String] {
        let args = config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p", ownedRuntime: true)
        return args.indices.filter { args[$0] == "--net-forward" }.map { args[$0 + 1] }
    }

    /// A VM forwards nothing until the user saves forwards; saved ones reach every launch of it.
    func testSavedPortForwardsReachTheRunner() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertEqual(forwardArguments(session.config), [])

        XCTAssertTrue(session.applyPortForwards(" 8080:80,3390 : 3389 ") == ("8080:80, 3390:3389", nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.portForwards, "8080:80, 3390:3389")
        var launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        XCTAssertEqual(forwardArguments(launch), ["8080:80", "3390:3389"])
        launch.virtioNet = false
        XCTAssertEqual(forwardArguments(launch), [], "no network, no forwards")

        // A typo is refused in one sentence and the saved forwards stay.
        let refused = session.applyPortForwards("8080:80, 3390")
        XCTAssertEqual(refused.text, "8080:80, 3390")
        XCTAssertNotNil(refused.message)
        XCTAssertEqual(library.latestConfiguration(for: saved).portForwards, "8080:80, 3390:3389")

        // Clearing the field restores the never-touched registration.
        XCTAssertTrue(session.applyPortForwards("") == ("", nil))
        XCTAssertNil(library.latestConfiguration(for: saved).portForwards)
    }

    func testAHandEditedEntryThatDoesNotParseIsLeftOut() {
        XCTAssertEqual(HvfPortForwards.saved("8080:80, junk, 0:22, 2222:22"),
                       [HvfPortForward(host: 8080, guest: 80), HvfPortForward(host: 2222, guest: 22)])
        XCTAssertNil(HvfPortForwards.parse("70000:80"))
    }

    func testACloneForwardsNothing() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        var template = fixture.config()
        template.portForwards = "8080:80"
        let bundle = fixture.root.appendingPathComponent("copy/bundle.vmbridge", isDirectory: true)
        XCTAssertNil(template.windowsCloneRegistration(slug: "copy", name: "Copy", bundle: bundle).portForwards)
    }
}
