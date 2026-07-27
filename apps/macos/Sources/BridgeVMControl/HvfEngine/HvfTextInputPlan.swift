import Foundation

/// Decides how a typed string reaches the guest.
///
/// A USB HID keyboard transmits physical key usages, not characters. There is
/// no usage code for "한", so the HID path can carry printable ASCII and
/// nothing else — the guest decoder rejects any other byte outright
/// (`xhci_hid_input/setup_input/actions.rs:75`, `ascii_text_action`).
///
/// The previous behaviour filtered the string down to `0x20...0x7e` and threw
/// the rest away, so typing Korean produced nothing at all and no diagnostic.
/// Non-ASCII instead goes through the guest clipboard, which carries
/// base64(UTF-8) intact (`bvagent.ps1:377`), followed by a paste.
struct HvfTextInputPlan: Equatable {
    /// Hex-encoded ASCII runs, already split to the 32-byte cap the guest
    /// token parser enforces.
    var hidChunks: [String]
    /// base64(UTF-8) of the whole string when it contains non-ASCII, else nil.
    var clipboardBase64: String?

    /// The guest rejects a `text-hex:` token longer than 32 bytes.
    static let hidChunkBytes = 32

    static func make(for value: String) -> HvfTextInputPlan {
        let bytes = Array(value.utf8)
        let isPrintableASCII = bytes.allSatisfy { (0x20...0x7e).contains($0) }

        if isPrintableASCII {
            return HvfTextInputPlan(hidChunks: hexChunks(bytes), clipboardBase64: nil)
        }

        // Mixed content goes entirely through the clipboard rather than being
        // split across two transports: interleaving HID and paste would let
        // the guest reorder them, and a scrambled string is worse than a slow
        // one.
        return HvfTextInputPlan(
            hidChunks: [],
            clipboardBase64: Data(value.utf8).base64EncodedString()
        )
    }

    private static func hexChunks(_ bytes: [UInt8]) -> [String] {
        guard !bytes.isEmpty else { return [] }
        return stride(from: 0, to: bytes.count, by: hidChunkBytes).map { start in
            bytes[start..<min(start + hidChunkBytes, bytes.count)]
                .map { String(format: "%02x", $0) }
                .joined()
        }
    }
}
