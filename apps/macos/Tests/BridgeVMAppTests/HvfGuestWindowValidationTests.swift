import XCTest
@testable import BridgeVMApp

final class HvfGuestWindowValidationTests: XCTestCase {
  func testInvalidHandlesNeverProduceMutationCommandsOrWindowRecords() {
    for id in ["0", "-1", "+1", "abc", "18446744073709551616", "1\nWINCLOSE 2"] {
      XCTAssertNil(HvfGuestWindowProtocol.focusCommand(id: id))
      XCTAssertNil(HvfGuestWindowProtocol.closeCommand(id: id))
      XCTAssertNil(HvfGuestWindowProtocol.boundsCommand(
        id: id, bounds: GuestToolsWindowBounds(x: 0, y: 0, width: 10, height: 10)))
      XCTAssertEqual(HvfGuestWindowProtocol.parseWindowList([
        "WIN \(id) 1 0 0 10 10 QQ==", "WINEND"
      ]), [])
    }
  }

  func testCanonicalHandleAndNegativeCoordinatesRemainUsable() {
    XCTAssertEqual(HvfGuestWindowProtocol.focusCommand(id: "00042"), "WINFOCUS 42")
    XCTAssertEqual(HvfGuestWindowProtocol.boundsCommand(
      id: "42", bounds: GuestToolsWindowBounds(x: -10, y: -20, width: 30, height: 40)),
      "WINBOUNDS 42 -10 -20 30 40")
  }

  func testInvalidProcessAndGeometryAreRejected() {
    for record in [
      "WIN 1 0 0 0 10 10 QQ==", "WIN 1 -1 0 0 10 10 QQ==",
      "WIN 1 4294967296 0 0 10 10 QQ==", "WIN 1 1 2147483648 0 10 10 QQ==",
      "WIN 1 1 0 0 2147483648 10 QQ==", "WIN 1 1 0 0 -1 10 QQ=="
    ] {
      XCTAssertEqual(HvfGuestWindowProtocol.parseWindowList([record, "WINEND"]), [])
    }
    for bounds in [
      GuestToolsWindowBounds(x: Int(Int32.max) + 1, y: 0, width: 10, height: 10),
      GuestToolsWindowBounds(x: 0, y: 0, width: Int(Int32.max) + 1, height: 10),
      GuestToolsWindowBounds(x: 0, y: 0, width: 0, height: 10)
    ] {
      XCTAssertNil(HvfGuestWindowProtocol.boundsCommand(id: "1", bounds: bounds))
    }
  }
}
