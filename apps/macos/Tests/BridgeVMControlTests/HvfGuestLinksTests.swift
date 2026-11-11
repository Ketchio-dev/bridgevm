#if canImport(AppKit)
import Foundation
import XCTest
@testable import BridgeVMControl

/// "Windows 링크를 Mac에서 열기" (HvfGuestLinks.swift): the setting, the guest registration lines and the
/// checks between a probe `BVAGENT OPENURL` line and the Mac browser.
@MainActor
final class HvfGuestLinksTests: XCTestCase {
    private typealias Links = HvfGuestLinks

    @MainActor
    private final class Opener: HvfURLOpener {
        var opened: [URL] = []
        var succeeds = true
        func open(_ url: URL) -> Bool { opened.append(url); return succeeds }
    }

    private func line(_ url: String, at date: Date) -> String {
        "BVAGENT OPENURL at=\(Int64(date.timeIntervalSince1970 * 1000)) \(Data(url.utf8).base64EncodedString())"
    }

    func testOnlyAnHttpLinkToANamedHostOtherThanTheMacOpens() {
        for url in ["https://example.com/?a=1&b=%22x%22#top", "HTTP://Example.COM/Path", "http://intranet:8080/",
                    "https://xn--r8jz45g.jp/%ED%95%9C", "http://10.0.0.5/", "http://128.0.0.1/", "http://[2001:db8::1]/"] {
            guard case let .open(opened, host) = Links.checked(url) else { return XCTFail(url) }
            XCTAssertEqual(opened.absoluteString, url)
            XCTAssertFalse(host.isEmpty, url)
        }
        for url in ["javascript:alert(1)", "file:///etc/passwd", "smb://server/share", "ftp://example.com/", "vnc://host",
                    "https:///path", "https://user@example.com/", "https://u:p@example.com/", "https://apple.com@evil.example/",
                    "https://x/a b", "https://x/\u{1}", "https://x/\u{a0}", "https://example.com/" + String(repeating: "a", count: 2029)] {
            guard case .refused = Links.checked(url) else { return XCTFail(url) }
        }
        XCTAssertEqual(Links.checked("https://apple.com@evil.example/"), .refused(host: "evil.example"), "the notice names the real host")
        for url in ["http://localhost/", "http://LOCALHOST./", "http://app.localhost:3000/", "http://127.0.0.1/", "http://127.1/",
                    "http://2130706433/", "http://0x7f000001/", "http://0.0.0.0:8080/", "http://[::1]/", "http://[::]/",
                    "http://[::ffff:127.0.0.1]/", "http://local%68ost/"] {
            guard case .loopback = Links.checked(url) else { return XCTFail(url) }
        }
    }

    /// run.log is read again from its start when the app reattaches: only a line logged now opens.
    func testOnlyAFreshWellFormedProbeLineCounts() {
        let now = Date(timeIntervalSince1970: 1_789_000_000)
        let url = "https://example.com/"
        XCTAssertNil(Links.outcome(of: "BVAGENT OPENURLX", now: now))
        XCTAssertNil(Links.outcome(of: "BVAGENT CLIPSYNC host->guest bytes=1", now: now))
        for offset in [-4.9, 0, 0.9] {
            guard case .open = Links.outcome(of: line(url, at: now.addingTimeInterval(offset)), now: now) else { return XCTFail("\(offset)") }
        }
        guard case .open = Links.outcome(of: line(url, at: now) + "\r", now: now) else { return XCTFail("a CRLF line") }
        for offset in [-5.1, -3600, 1.1] {
            XCTAssertEqual(Links.outcome(of: line(url, at: now.addingTimeInterval(offset)), now: now), .stale, "\(offset)")
        }
        let b64 = Data(url.utf8).base64EncodedString(), ms = Int64(now.timeIntervalSince1970 * 1000)
        for bad in ["BVAGENT OPENURL refused reason=scheme", "BVAGENT OPENURL at=x \(b64)", "BVAGENT OPENURL at=+\(ms) \(b64)",
                    "BVAGENT OPENURL at=\(ms) \(b64) extra", "BVAGENT OPENURL at=\(ms) not-base64", "BVAGENT OPENURL at=\(ms)",
                    line("https://x/" + String(repeating: "a", count: 2039), at: now)] {
            XCTAssertEqual(Links.outcome(of: bad, now: now), .refused(host: nil), bad)
        }
        XCTAssertEqual(Links.outcome(of: line("http://127.0.0.1:9/", at: now), now: now), .loopback(host: "127.0.0.1"))
    }

    func testTheSettingIsSavedOnlyWhileStoppedAndOffAfterOnIsSavedAsFalse() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertFalse(session.guestLinksWanted, "never set is off")
        XCTAssertTrue(session.applyGuestLinksToggle(false) == (false, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).openGuestLinksOnMac, "off while never on stays absent")

        session.connectionState = .connected(host: "guest")
        XCTAssertEqual(session.applyGuestLinksToggle(true).message, "VM을 완전히 중지한 뒤 링크 설정을 변경하세요.")
        XCTAssertNil(library.latestConfiguration(for: saved).openGuestLinksOnMac)

        session.connectionState = .stopped
        XCTAssertTrue(session.applyGuestLinksToggle(true) == (true, nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.openGuestLinksOnMac, true)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: latest) === session, "rebound in place")
        XCTAssertNil(library.operationError)
        XCTAssertTrue(session.guestLinksWanted)
        XCTAssertTrue(session.applyGuestLinksToggle(false) == (false, nil))
        XCTAssertEqual(library.latestConfiguration(for: saved).openGuestLinksOnMac, false, "the next agent start removes it")
    }

    /// On: every agent start registers the handler and starts the link watch. Turned off: the registration's
    /// removal once per app session. Never set: nothing at all.
    func testAgentStartRegistersAndWatchesWhenOnAndRemovesOnceWhenTurnedOff() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let control = URL(fileURLWithPath: session.config.ctlFilePath)
        try FileManager.default.createDirectory(at: control.deletingLastPathComponent(), withIntermediateDirectories: true)
        func sent() -> [String] { ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n").map(String.init) }
        func agentStarts() { session.connectionState = .connected(host: "guest"); session.handle(.serviceStart(tMs: 1)) }
        func change(_ enabled: Bool) {
            session.connectionState = .stopped
            XCTAssertTrue(session.applyGuestLinksToggle(enabled) == (enabled, nil))
        }
        let (register, watch, remove) = (Links.registerCommands, Links.watchCommand, Links.removeCommand)

        agentStarts()
        XCTAssertFalse(sent().contains { register.contains($0) || $0 == watch || $0 == remove }, "never set: nothing")
        let before = sent().count
        change(true)
        agentStarts()
        agentStarts()  // a guest restart: registered and watched again
        XCTAssertEqual(Array(sent().dropFirst(before)), register + [watch] + register + [watch])
        change(false)
        agentStarts()
        agentStarts()
        XCTAssertEqual(Array(sent().dropFirst(before + 6)), [remove], "removed once per app session")
        change(true)
        agentStarts()
        change(false)
        agentStarts()
        XCTAssertEqual(Array(sent().dropFirst(before + 7)), register + [watch, remove])
        XCTAssertNil(library.operationError)
    }

    /// The registration is per user, runs the handler with the link as the one `-File` argument, and writes it
    /// unchanged; the removal deletes only registry keys.
    func testTheRegistrationLinesRunTheHandlerAsDataForThisUserOnly() throws {
        func script(_ command: String) throws -> String {
            let b64 = try XCTUnwrap(command.split(separator: " ").last)
            let bytes = try XCTUnwrap(Data(base64Encoded: String(b64)))
            let units = stride(from: 0, to: bytes.count, by: 2).map { UInt16(bytes[$0]) | UInt16(bytes[$0 + 1]) << 8 }
            return try XCTUnwrap(String(utf16CodeUnits: units, count: units.count))
        }
        let handlerB64 = Data(Links.handlerScript.utf8).base64EncodedString()
        let encoded = "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand "
        XCTAssertEqual(Links.registerCommands.count, 2)
        XCTAssertTrue(Links.registerCommands[0].hasPrefix("set BVLINK_HANDLER=\(handlerB64)&& " + encoded))
        for command in Links.registerCommands + [Links.removeCommand] {
            XCTAssertTrue(command.contains(encoded))
            // bvagent.ps1 runs a RUN line as `& cmd.exe /c $cmd`, and cmd.exe refuses a line over 8,191.
            let cmdLine = #""C:\Windows\system32\cmd.exe" /c ""# + command + "\""
            XCTAssertTrue(cmdLine.count <= 8_191, "\(cmdLine.count) characters")
            XCTAssertEqual(try HvfGuestCommand.normalizeShell(command).get(), command)
        }
        let handler = try script(Links.registerCommands[0]), register = try script(Links.registerCommands[1])
        let remove = try script(Links.removeCommand)
        XCTAssertTrue(handler.contains("[Convert]::FromBase64String($env:BVLINK_HANDLER)"))
        XCTAssertTrue(handler.contains("ReparsePoint"), "an elevated write never goes through a reparse point")
        XCTAssertTrue(register.contains(#"-WindowStyle Hidden -File "' + $handler + '" "%1"'"#), "the link is the one argument after -File")
        XCTAssertTrue([register, remove].allSatisfy { $0.contains("[Microsoft.Win32.Registry]::CurrentUser") })
        for text in [handler, register, remove] {
            XCTAssertFalse(text.range(of: "LocalMachine|HKLM|Remove-Item|\\.Delete\\(|Invoke-Expression|cmd", options: [.regularExpression, .caseInsensitive]) != nil, text)
        }
        XCTAssertTrue(Links.handlerScript.allSatisfy(\.isASCII))
        XCTAssertNil(Links.handlerScript.range(of: "Invoke-Expression|-Command|cmd\\b|param\\(", options: [.regularExpression, .caseInsensitive]))
    }

    /// A reattached session reads run.log from its start: a link logged before the session existed is never
    /// opened, even inside the freshness window.
    func testALinkLoggedBeforeTheSessionIsNeverOpened() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let opener = Opener()
        session.guestLinkOpener = opener
        XCTAssertTrue(session.applyGuestLinksToggle(true) == (true, nil))
        let start = session.guestLinksNotBefore
        session.guestLinkReported(line("https://example.com/once", at: start.addingTimeInterval(-0.5)),
                                  now: start.addingTimeInterval(1))
        XCTAssertTrue(opener.opened.isEmpty, "logged before this session: run.log read again, not a click")
        session.guestLinkReported(line("https://example.com/new", at: start.addingTimeInterval(0.5)),
                                  now: start.addingTimeInterval(1))
        XCTAssertEqual(opener.opened.map(\.absoluteString), ["https://example.com/new"])
    }

    func testLinksOpenAtMostOncePerSecondAndOnlyWithTheSettingOn() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let opener = Opener()
        session.guestLinkOpener = opener
        let now = Date()

        session.guestLinkReported(line("https://example.com/off", at: now), now: now)
        XCTAssertTrue(opener.opened.isEmpty, "off: ignored")
        XCTAssertNil(session.guestLinkNotice.shown)

        XCTAssertTrue(session.applyGuestLinksToggle(true) == (true, nil))
        session.guestLinkReported(line("https://example.com/?token=secret", at: now), now: now)
        session.guestLinkReported(line("https://example.org/", at: now), now: now.addingTimeInterval(0.5))
        XCTAssertEqual(opener.opened.map(\.absoluteString), ["https://example.com/?token=secret"])
        XCTAssertEqual(session.guestLinkNotice.shown, .tooFrequent(host: "example.org"))
        XCTAssertEqual(session.guestLinkNotice.text?.contains("token"), false, "the host only")
        session.guestLinkReported(line("https://example.org/", at: now), now: now.addingTimeInterval(1))
        XCTAssertEqual(opener.opened.count, 2)
        XCTAssertNil(session.guestLinkNotice.shown, "a link opened clears the notice")

        session.guestLinkReported(line("http://localhost:8080/", at: now), now: now.addingTimeInterval(3))
        XCTAssertEqual(session.guestLinkNotice.shown, .loopback(host: "localhost"))
        session.guestLinkReported(line("https://stale.example/", at: now.addingTimeInterval(-10)), now: now.addingTimeInterval(4))
        opener.succeeds = false
        session.guestLinkReported(line("https://example.net/", at: now.addingTimeInterval(5)), now: now.addingTimeInterval(5))
        XCTAssertEqual(session.guestLinkNotice.shown, .failed(host: "example.net"))
        XCTAssertEqual(opener.opened.map(\.host), ["example.com", "example.org", "example.net"], "loopback and stale never opened")

        // The event feed names the host only.
        session.handle(.unknown(line("https://example.com/?token=secret", at: Date())))
        let shown = try XCTUnwrap(session.events.last?.displayText)
        XCTAssertTrue(shown.hasPrefix("BVAGENT OPENURL at=") && shown.hasSuffix(" host=example.com"), shown)
    }
}
#endif
