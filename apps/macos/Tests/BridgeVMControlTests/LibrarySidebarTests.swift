import XCTest
@testable import BridgeVMControl

@MainActor
final class LibrarySidebarTests: XCTestCase {
    /// VoiceOver reads the Virtual machines button's count badge with its unit, not as a bare number.
    func testVMCountValueNamesItsUnit() {
        XCTAssertEqual(LibrarySidebar.vmCountText(0), "0 VMs")
        XCTAssertEqual(LibrarySidebar.vmCountText(1), "1 VM")
        XCTAssertEqual(LibrarySidebar.vmCountText(3), "3 VMs")
    }
}
