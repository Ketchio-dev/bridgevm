import Foundation

/// The guest-to-host clipboard limit of the guest-tools "Copy to Mac clipboard"
/// route: the probe's GUEST_CLIP_MAX_BYTES (decision of 2026-10-23, applied to
/// this route by its follow-up). Text over 8 MiB of UTF-8 is refused whole and
/// never truncated, so the Mac pasteboard keeps what it holds.
extension GuestClipboardSnapshot {
  static let maxHostCopyUTF8Bytes = 8 * 1024 * 1024

  /// Whether this text may be written to the Mac pasteboard.
  var isCopyableToHost: Bool {
    !text.isEmpty && text.utf8.count <= Self.maxHostCopyUTF8Bytes
  }

  /// The panel's preview, which shows only the size of a refused text.
  var previewText: String {
    if text.isEmpty { return "Empty text" }
    if isCopyableToHost { return text }
    return "Not copied: \(text.utf8.count) bytes is over the 8 MiB limit"
  }
}
