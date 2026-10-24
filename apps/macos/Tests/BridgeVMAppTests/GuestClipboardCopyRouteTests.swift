import XCTest

@testable import BridgeVMApp

/// The guest-tools "Copy to Mac clipboard" route (follow-up of 2026-10-23).
/// The daemon reports the guest's last clipboard text under `clipboard`, and
/// the route copies it with the guest-to-host limit of the probe's pasteboard
/// route: over 8 MiB of UTF-8 it is refused whole, never truncated.
final class GuestClipboardCopyRouteTests: XCTestCase {
  /// Records writes instead of mutating the real `NSPasteboard.general`.
  private final class FakeHostPasteboard: HostPasteboardWriting {
    private(set) var writtenStrings: [String] = []

    func writeString(_ string: String) {
      writtenStrings.append(string)
    }
  }

  /// A guest_tools_status reply whose runtime is the daemon's own serialization
  /// of GuestToolsRuntimeMetadata (pinned by bridgevm-storage's
  /// guest_tools_clipboard_key test).
  private func status(runtime: String) throws -> GuestToolsStatus {
    let json = """
      {"type":"guest_tools_status","status":{"vm":"dev","tools":"required",\
      "token_created_at_unix":1710000000,"capabilities":[],\
      "approved_shared_folders":[],"runtime":\(runtime)}}
      """
    return try JSONDecoder().decode(
      DaemonGuestToolsStatusResponse.self, from: Data(json.utf8)
    ).status.guestToolsStatus
  }

  func testTheDaemonsClipboardFieldReachesTheCopyRoute() throws {
    let runtime = """
      {"connected":true,"guest_os":"linux","agent_version":"0.1.0",\
      "capabilities":["clipboard"],"last_heartbeat_at_unix":1710000060,\
      "guest_ip_addresses":[],"shared_folders":[],"metrics":null,\
      "clipboard":{"text":"guest copied text","updated_at_unix":1710000064},\
      "updated_at_unix":1710000061}
      """
    let clipboard = try XCTUnwrap(try status(runtime: runtime).runtime).lastClipboard
    XCTAssertEqual(
      clipboard, GuestClipboardSnapshot(text: "guest copied text", updatedAtUnix: 1_710_000_064))
    let pasteboard = FakeHostPasteboard()
    XCTAssertTrue(copyGuestClipboardToHost(clipboard, into: pasteboard))
    XCTAssertEqual(pasteboard.writtenStrings, ["guest copied text"])
  }

  func testAKeyTheDaemonNeverWritesIsNotRead() throws {
    let runtime = """
      {"connected":true,"capabilities":[],"guest_ip_addresses":[],\
      "last_clipboard":{"text":"stale","updated_at_unix":1},"updated_at_unix":2}
      """
    XCTAssertNil(try XCTUnwrap(try status(runtime: runtime).runtime).lastClipboard)
  }

  func testGuestTextOneByteOver8MiBIsRefusedWhole() {
    let pasteboard = FakeHostPasteboard()
    let text = String(repeating: "x", count: 8 * 1024 * 1024 + 1)
    let snapshot = GuestClipboardSnapshot(text: text, updatedAtUnix: 1)
    XCTAssertFalse(copyGuestClipboardToHost(snapshot, into: pasteboard))
    XCTAssertTrue(pasteboard.writtenStrings.isEmpty, "\(pasteboard.writtenStrings.count) writes")
  }

  func testTheLimitCountsUTF8BytesNotCharacters() {
    let pasteboard = FakeHostPasteboard()
    // 2,796,203 Hangul syllables: 8,388,609 bytes of UTF-8.
    let text = String(repeating: "가", count: 8 * 1024 * 1024 / 3 + 1)
    let snapshot = GuestClipboardSnapshot(text: text, updatedAtUnix: 1)
    XCTAssertFalse(copyGuestClipboardToHost(snapshot, into: pasteboard))
    XCTAssertTrue(pasteboard.writtenStrings.isEmpty, "\(pasteboard.writtenStrings.count) writes")
  }

  func testGuestTextOfExactly8MiBIsCopiedWhole() {
    let pasteboard = FakeHostPasteboard()
    let text = String(repeating: "x", count: 8 * 1024 * 1024)
    let snapshot = GuestClipboardSnapshot(text: text, updatedAtUnix: 1)
    XCTAssertTrue(copyGuestClipboardToHost(snapshot, into: pasteboard))
    let written = pasteboard.writtenStrings.map(\.utf8.count)
    XCTAssertEqual(written, [8 * 1024 * 1024])
  }
}
