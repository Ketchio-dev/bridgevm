import Foundation
import XCTest
import BridgeVMWindowProtocol

final class HvfSharedWindowRecordTests: XCTestCase {
  func testProductTargetUsesValidatedWindowRecord() throws {
    let title = "\u{ba54}\u{baa8}\u{c7a5}"
    let encoded = Data(title.utf8).base64EncodedString()
    let record = try XCTUnwrap(GuestWindowRecord(protocolLine: "WIN 00042 7 -10 20 640 480 \(encoded)"))
    XCTAssertEqual(record.id, "42")
    XCTAssertEqual(record.processID, 7)
    XCTAssertEqual(record.x, -10)
    XCTAssertEqual(record.width, 640)
    XCTAssertEqual(record.title, title)
  }

  func testProductTargetRejectsMalformedIdentityAndGeometry() {
    for line in [
      "WIN bad 7 0 0 10 10 QQ==", "WIN 1 0 0 0 10 10 QQ==",
      "WIN 1 7 0 0 0 10 QQ==", "WIN 1 7 2147483648 0 10 10 QQ==",
      "WIN 1 7 0 0 10 10 /w==", "WIN 1 7 0 0 10 10 ???",
      "WIN 1 7 0 0 10 10"
    ] {
      XCTAssertNil(GuestWindowRecord(protocolLine: line))
    }
  }
}
