import XCTest
@testable import BridgeVMProductE2E

/// The probe never prints guest clipboard text (follow-up of 2026-10-23): its
/// CLIPGET reply is one line with the text's byte length and the first 16 hex
/// digits of its SHA-256. The journey proves its clipboard read by matching
/// both against the text it set.
final class T17ClipboardFingerprintReplyTests: XCTestCase {
    /// The journey's clipboard text for nonce n1: 42 bytes of UTF-8.
    private let body = "브리지VM T17 클립보드 왕복 v1\nn1\n"
    /// `shasum -a 256` of that text: d198fb5f41e41381...
    private let reply = "BVAGENT CLIP CLIPGET bytes=42 sha256=d198fb5f41e41381"

    func testTheTextsLengthAndSHA256PrefixProveTheRead() {
        XCTAssertTrue(T17ControlReply.succeeded(tail: reply + "\n", command: "CLIPGET", marker: body))
        XCTAssertTrue(T17ControlReply.succeeded(tail: "BVAGENT SHARE x\r\n\(reply)\r\n",
            command: "CLIPGET", marker: body))
    }

    func testAnyOtherReplyDoesNot() {
        let others = [
            "BVAGENT CLIP CLIPGET bytes=43 sha256=d198fb5f41e41381",
            "BVAGENT CLIP CLIPGET bytes=42 sha256=d198fb5f41e41380",
            // The same text with CRLF line ends is another text.
            "BVAGENT CLIP CLIPGET bytes=44 sha256=8d2c5d4aa523f32b",
            "BVAGENT CLIP CLIPGET base64-error=Byte payload-bytes=4",
            "BVAGENT CLIP CLIPGET\n\(body)\nBVAGENT END CLIPGET",
            "BVAGENT CMD CLIPGET exit=0\n\(body)\nBVAGENT END CLIPGET",
            reply + " extra",
        ]
        for tail in others {
            XCTAssertFalse(T17ControlReply.succeeded(tail: tail + "\n", command: "CLIPGET", marker: body),
                "accepted a \(tail.utf8.count) byte reply")
        }
        XCTAssertFalse(T17ControlReply.succeeded(tail: reply + "\n", command: "CLIPGET", marker: ""))
    }
}
