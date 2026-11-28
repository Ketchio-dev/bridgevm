import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardStatusAnnouncementTests: XCTestCase {
    /// A start, a pause and a failure are announced once; passing states and a return to the same state are not.
    func testAnnouncesOnlyANewSettledStatus() {
        var announced: String? = "Stopped"
        var spoken: [String] = []
        for status in ["Preparing to start", "Booting Windows", "Running", "Working", "Running", "Paused",
                       "Running", "Needs attention", "Shutting down", "Stopped"] {
            if let message = LibraryDashboardDetail.statusAnnouncement(name: "Windows 11", status: status, after: announced) {
                announced = status
                spoken.append(message)
            }
        }
        XCTAssertEqual(spoken, ["Windows 11: Running", "Windows 11: Paused", "Windows 11: Running",
                                "Windows 11: Needs attention", "Windows 11: Stopped"])
        XCTAssertEqual(LibraryDashboardDetail.statusAnnouncement(name: "Windows 11", status: "Running", after: nil),
                       "Windows 11: Running")
    }
}
