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

    /// Forwards could change only with Windows shut down. With Guest Tools connected they now change at once
    /// (the VMM opens and closes its Mac ports), and vm.json follows at the stop.
    func testForwardsChangeWhileWindowsRunsAndAreSavedAtTheStop() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertTrue(session.applyPortForwards("8080:80") == ("8080:80", nil))
        let control = URL(fileURLWithPath: session.config.ctlFilePath)
        try FileManager.default.createDirectory(at: control.deletingLastPathComponent(), withIntermediateDirectories: true)
        session.connectionState = .connected(host: "guest")
        session.handle(.serviceStart(tMs: 1))
        XCTAssertTrue(session.portForwardsLive)
        session.handle(.unknown("BVNET forward-off host=8080 guest=80"))
        XCTAssertNotNil(session.portForwardNotice.text)

        XCTAssertTrue(session.applyPortForwards("8080:80,3390:3389") == ("8080:80, 3390:3389", nil))
        let sent = ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n")
            .filter { $0.hasPrefix("BVNETFORWARD") }
        XCTAssertEqual(sent, ["BVNETFORWARD 8080:80, 3390:3389"])
        XCTAssertNil(session.portForwardNotice.text, "the VMM says again which ones it couldn’t open")
        XCTAssertEqual(HvfPortForwards.text(session.config.portForwards), "8080:80, 3390:3389", "what Settings shows")
        XCTAssertEqual(library.latestConfiguration(for: saved).portForwards, "8080:80", "vm.json waits for the stop")
        // Windows restarts: a new VM process opens the forwards of the start, so the change goes again.
        session.handle(.unknown(HvfGuestResetRecord.processRecreation))
        session.handle(.serviceStart(tMs: 2))
        let again = ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n")
            .filter { $0.hasPrefix("BVNETFORWARD") }
        XCTAssertEqual(again, ["BVNETFORWARD 8080:80, 3390:3389", "BVNETFORWARD 8080:80, 3390:3389"])
        session.connectionState = .stopped
        session.markStopped()
        XCTAssertEqual(library.latestConfiguration(for: saved).portForwards, "8080:80, 3390:3389")
        XCTAssertTrue(session.liveControlLines.isEmpty, "the next start opens what vm.json holds")
        XCTAssertNil(library.operationError)
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
        // 80:80 was saved, then the VMM couldn't open Mac port 80 and the notice blamed another app.
        XCTAssertTrue(session.applyPortForwards("8080:80, 443:443") == ("8080:80, 443:443", HvfPortForwards.reservedMessage))
        XCTAssertNil(library.latestConfiguration(for: saved).portForwards)
        session.handle(.unknown("BVNET forward-off host=80 guest=80"))
        XCTAssertEqual(session.portForwardNotice.text,
            "Mac port 80 doesn’t reach Windows port 80 because macOS keeps ports below 1024 for itself. Choose a Mac port from 1024 up in Settings, then restart Windows.")
        session.resetObservedRuntimeState(clearEvents: false)

        // The line net_nat/port_forward.rs forward_off_line writes (its test checks the same text).
        for line in ["BVNET forward-off host=8080 guest=80", "BVNET forward-off host=8080 guest=80\r"] {
            session.handle(.unknown(line))
        }
        XCTAssertEqual(session.portForwardNotice.text,
            "Another app or another BridgeVM VM is using Mac port 8080, so it doesn’t reach Windows port 80. Quit that app or shut down that VM, or choose another Mac port in Settings, then restart Windows.")
        session.handle(.unknown("BVNET forward-off host=3390 guest=3389"))
        session.handle(.unknown("BVNET forward-off host=x guest=1"))
        XCTAssertEqual(session.portForwardNotice.text,
            "Other apps or other BridgeVM VMs are using Mac ports 8080, 3390, so they don’t reach Windows. Quit those apps or shut down those VMs, or choose other Mac ports in Settings, then restart Windows.")
        session.resetObservedRuntimeState(clearEvents: false)
        XCTAssertNil(session.portForwardNotice.text, "a new run opens the ports again")

        // A forward whose Windows port refuses or never answers (net_nat/port_forward.rs): the Mac
        // client only saw its connection close.
        session.handle(.unknown("BVNET forward-unanswered guest=3389"))
        XCTAssertEqual(session.portForwardNotice.text,
            "Windows isn’t accepting forwarded connections on port 3389. Start the app that should listen there, or allow it in Windows Defender Firewall.")
        session.handle(.unknown("BVNET forward-answered guest=3389"))
        XCTAssertNil(session.portForwardNotice.text, "a connection that opens clears it")
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
        XCTAssertEqual(notice.text?.hasPrefix(HvfPortForwardNotice.dnsUnreachableText + " Another app or another BridgeVM VM is using Mac port 8080"), true)
        notice.consume("BVNET dns-reachable")
        XCTAssertEqual(notice.text?.hasPrefix("Another app or another BridgeVM VM is using Mac port 8080"), true)
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
