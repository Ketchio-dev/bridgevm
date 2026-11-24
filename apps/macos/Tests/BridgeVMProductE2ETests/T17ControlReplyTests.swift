import XCTest
@testable import BridgeVMProductE2E

final class T17ControlReplyTests: XCTestCase {
    func testClipboardSetMatchesItsFingerprintOrExactLegacyReply() {
        let fingerprint = "BVAGENT CLIPSET bytes=1 sha256=ca978112ca1bbdca"
        let acknowledged = "\(fingerprint) -> OK CLIPSET\r\n"
        XCTAssertTrue(T17ControlReply.succeeded(tail: acknowledged, command: "CLIPSET YQ==", marker: "OK CLIPSET"))
        XCTAssertFalse(T17ControlReply.succeeded(tail: acknowledged, command: "CLIPSET Yg==", marker: "OK CLIPSET"))
        XCTAssertFalse(T17ControlReply.succeeded(tail: acknowledged + "\(fingerprint) -> ERR CLIPSET\n",
            command: "CLIPSET YQ==", marker: "OK CLIPSET"))
        XCTAssertTrue(T17ControlReply.succeeded(tail: "BVAGENT CLIPSET YQ== -> OK CLIPSET\n",
            command: "CLIPSET YQ==", marker: "OK CLIPSET"))
        XCTAssertFalse(T17ControlReply.succeeded(tail: "BVAGENT CLIPSET YQ== -> ERR CLIPSET busy\n",
            command: "CLIPSET YQ==", marker: "OK CLIPSET"))
    }

    func testClipboardReadNeedsItsOwnCompleteFrame() {
        XCTAssertTrue(T17ControlReply.succeeded(tail: "BVAGENT CLIP CLIPGET bytes=5 sha256=cd42404d52ad55cc\r\n",
            command: "CLIPGET", marker: "value"))
        XCTAssertFalse(T17ControlReply.succeeded(tail: "BVAGENT CLIP CLIPGET\nvalue\nBVAGENT END other\n",
            command: "CLIPGET", marker: "value"))
    }

    func testCommandRequiresSuccessfulExactHeaderAndEnd() {
        for exit in [0, 1] {
            XCTAssertEqual(T17ControlReply.succeeded(tail: "BVAGENT CMD task exit=\(exit)\nnonce\nBVAGENT END task\n",
                command: "task", marker: "nonce"), exit == 0)
        }
        XCTAssertFalse(T17ControlReply.succeeded(tail: "BVAGENT CMD task exit=0\nnonce\n",
            command: "task", marker: "nonce"))
    }

    func testUnrelatedOutputCannotSupplyTheMarker() {
        XCTAssertFalse(T17ControlReply.succeeded(tail: "nonce\nBVAGENT CMD task exit=0\nother\nBVAGENT END task\n",
            command: "task", marker: "nonce"))
        XCTAssertFalse(T17ControlReply.succeeded(tail: "BVAGENT CLIPSET Yg== -> OK CLIPSET\nBVAGENT END other\n",
            command: "CLIPSET YQ==", marker: "OK CLIPSET"))
    }
}
