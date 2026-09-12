import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerReleaseEncodingTests: XCTestCase {
    func testCommonAndSeparateReleaseCountsRemainOne() {
        for command in ["releaseall:0x32767", "release:0x0", "rightrelease:32767x0"] {
            let encoding = HvfGuestInputEncoding(.pointer(command))
            XCTAssertEqual(encoding?.verb, "POINTERINPUT")
            XCTAssertEqual(encoding?.insertedEventCount, 1)
            XCTAssertEqual(encoding?.base64, Data(command.utf8).base64EncodedString())
        }
        for command in ["releaseall:32768x0", "releaseall:-1x0", "releaseall:0x0\n"] {
            XCTAssertNil(HvfGuestInputEncoding(.pointer(command)))
        }
    }

    func testVersionTwoWithoutCommonReleaseIsRejected() {
        let now = Date(timeIntervalSince1970: 2_000)
        var request = HvfInputCapabilitiesRequest(now: now)
        let id = request.command.split(separator: " ")[1]
        let lines = ["BVAGENT CMD \(request.command) exit=0", "BVINPUT_CAPS \(id) 2 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                     "BVAGENT END \(request.command)"]
        XCTAssertEqual(request.consume(lines: lines, now: now), .failed(.invalidReceipt))
    }
}
