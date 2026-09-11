import XCTest
@testable import BridgeVMProductE2E

final class T17ControlReplyTests: XCTestCase {
    func testRawClipboardSetDoesNotRequireAnInventedEndRecord() {
        XCTAssertTrue(T17ControlReply.succeeded(tail: "BVAGENT CLIPSET YQ== -> OK CLIPSET\n",
            command: "CLIPSET YQ==", marker: "OK CLIPSET"))
        XCTAssertFalse(T17ControlReply.succeeded(tail: "BVAGENT CLIPSET YQ== -> ERR CLIPSET busy\n",
            command: "CLIPSET YQ==", marker: "OK CLIPSET"))
    }

    func testClipboardReadNeedsItsOwnCompleteFrame() {
        XCTAssertTrue(T17ControlReply.succeeded(tail: "BVAGENT CLIP CLIPGET\r\nvalue\r\nBVAGENT END CLIPGET\r\n",
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
