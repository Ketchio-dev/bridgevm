import AppKit

/// Seam for writing text to a host pasteboard.
///
/// Abstracted behind a protocol so the guest->host clipboard copy logic can be
/// unit-tested with a fake instead of mutating the real `NSPasteboard.general`.
protocol HostPasteboardWriting {
  func writeString(_ string: String)
}

/// `NSPasteboard`-backed conformer used in the running app.
struct SystemHostPasteboard: HostPasteboardWriting {
  private let pasteboard: NSPasteboard

  init(pasteboard: NSPasteboard = .general) {
    self.pasteboard = pasteboard
  }

  func writeString(_ string: String) {
    pasteboard.clearContents()
    pasteboard.setString(string, forType: .string)
  }
}

/// Pure helper: write a guest clipboard snapshot's text into the host pasteboard.
///
/// Returns `true` when text was written, `false` when there was nothing safe to
/// write: no snapshot, empty text (a guest cannot blank the host pasteboard
/// here) or text over the 8 MiB guest-to-host limit, refused whole, never cut.
@discardableResult
func copyGuestClipboardToHost(
  _ snapshot: GuestClipboardSnapshot?,
  into pasteboard: HostPasteboardWriting
) -> Bool {
  guard let snapshot, snapshot.isCopyableToHost else {
    return false
  }
  pasteboard.writeString(snapshot.text)
  return true
}
