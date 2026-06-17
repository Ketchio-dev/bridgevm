import XCTest

@testable import BridgeVMApp

final class HostPasteboardTests: XCTestCase {
  /// Records writes instead of mutating the real `NSPasteboard.general`.
  private final class FakeHostPasteboard: HostPasteboardWriting {
    private(set) var writtenStrings: [String] = []

    func writeString(_ string: String) {
      writtenStrings.append(string)
    }
  }

  func testCopiesNonEmptyGuestClipboardTextAndReturnsTrue() {
    let pasteboard = FakeHostPasteboard()
    let snapshot = GuestClipboardSnapshot(text: "hello from guest", updatedAtUnix: 1_710_000_000)

    let wrote = copyGuestClipboardToHost(snapshot, into: pasteboard)

    XCTAssertTrue(wrote)
    XCTAssertEqual(pasteboard.writtenStrings, ["hello from guest"])
  }

  func testNilSnapshotWritesNothingAndReturnsFalse() {
    let pasteboard = FakeHostPasteboard()

    let wrote = copyGuestClipboardToHost(nil, into: pasteboard)

    XCTAssertFalse(wrote)
    XCTAssertTrue(pasteboard.writtenStrings.isEmpty)
  }

  func testEmptyTextWritesNothingAndReturnsFalse() {
    let pasteboard = FakeHostPasteboard()
    let snapshot = GuestClipboardSnapshot(text: "", updatedAtUnix: 1_710_000_000)

    let wrote = copyGuestClipboardToHost(snapshot, into: pasteboard)

    XCTAssertFalse(wrote)
    XCTAssertTrue(pasteboard.writtenStrings.isEmpty)
  }
}
