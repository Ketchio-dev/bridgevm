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

    /// A Mac port another app holds (or a second rule for the same Mac port) was only a run.log line:
    /// the VM ran without that forward and nothing said so.
    func testAForwardTheMacCannotOpenIsNamedAndARepeatedMacPortIsRefused() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertTrue(session.applyPortForwards("8080:80, 8080:8080") == ("8080:80, 8080:8080", HvfPortForwards.repeatedMessage))
        XCTAssertNil(library.latestConfiguration(for: saved).portForwards)

        // The line net_nat/port_forward.rs forward_off_line writes (its test checks the same text).
        for line in ["BVNET forward-off host=8080 guest=80", "BVNET forward-off host=8080 guest=80\r"] {
            session.handle(.unknown(line))
        }
        XCTAssertEqual(session.portForwardNotice.text,
            "Mac port 8080 is already in use, so it doesn’t reach Windows port 80. Quit the app using it, or choose another Mac port in Settings, then restart Windows.")
        session.handle(.unknown("BVNET forward-off host=3390 guest=3389"))
        session.handle(.unknown("BVNET forward-off host=x guest=1"))
        XCTAssertEqual(session.portForwardNotice.text,
            "Mac ports 8080, 3390 are already in use, so they don’t reach Windows. Quit the apps using them, or choose other Mac ports in Settings, then restart Windows.")
        session.resetObservedRuntimeState(clearEvents: false)
        XCTAssertNil(session.portForwardNotice.text, "a new run opens the ports again")
    }

    /// The lines net_nat/host_socket.rs prints when sends to the Mac's DNS server on its own network
    /// fail as they do with Local Network access off, and when a reply comes back. Windows resolved
    /// no names and nothing said why.
    func testUnreachableDNSIsShownUntilAReplyComesBack() {
        let notice = HvfPortForwardNotice()
        notice.consume("BVNET dns-unreachable\r")
        XCTAssertEqual(notice.text, HvfPortForwardNotice.dnsUnreachableText)
        XCTAssertTrue(notice.text?.contains("Privacy & Security > Local Network") == true)
        notice.consume("BVNET forward-off host=8080 guest=80")
        XCTAssertEqual(notice.text?.hasPrefix(HvfPortForwardNotice.dnsUnreachableText + " Mac port 8080"), true)
        notice.consume("BVNET dns-reachable")
        XCTAssertEqual(notice.text?.hasPrefix("Mac port 8080"), true)
        notice.consume("BVNET dns-unreachable")
        notice.reset()
        XCTAssertNil(notice.text)
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
        template.startWithBridgeVM = true
        XCTAssertNil(template.windowsCloneRegistration(slug: "copy", name: "Copy", bundle: bundle).startWithBridgeVM,
                     "a copy does not also start when BridgeVM opens")
    }
}
