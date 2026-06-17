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
/// write (nil snapshot or empty text). Never writes empty/nil text, so a guest
/// cannot silently blank the host pasteboard through this path.
@discardableResult
func copyGuestClipboardToHost(
  _ snapshot: GuestClipboardSnapshot?,
  into pasteboard: HostPasteboardWriting
) -> Bool {
  guard let text = snapshot?.text, !text.isEmpty else {
    return false
  }
  pasteboard.writeString(text)
  return true
}
