import XCTest
@testable import BridgeVMProductE2E

final class T17GuestWorkloadOutputTests: XCTestCase {
    private var share: URL!
    private var error: URL { share.appendingPathComponent("t17-error-0123456789ab.txt") }

    override func setUpWithError() throws {
        share = FileManager.default.temporaryDirectory.appendingPathComponent("t17-output-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: share, withIntermediateDirectories: true)
    }
    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: share) }

    func testOnlyAnExactActionAndExceptionTypeIsReported() throws {
        let good = "action=Audio error=System.IO.DirectoryNotFoundException"
        for (body, expected) in [(good + "\n", good), (good, nil), (good + "\nmore\n", nil), ("action=Audio error=\n", nil),
                                 ("action=Au dio error=X\n", nil), ("action=Audio error=Bad Type\n", nil), ("\u{1b}" + good + "\n", nil)] {
            try Data(body.utf8).write(to: error)
            XCTAssertEqual(T17GuestWorkloadOutput.failure(error), expected, body.debugDescription)
        }
    }

    func testTimeoutReportsTheNoteAndTheGuestFailure() throws {
        try Data("action=Audio error=System.IO.DirectoryNotFoundException\n".utf8).write(to: error)
        XCTAssertThrowsError(try T17GuestWorkloadOutput.await(share: share, name: "t17-audio-0123456789ab.txt", prefix: "0123456789ab",
                                                              timeout: 0.3, note: { "stage note" })) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest workload did not produce t17-audio-0123456789ab.txt (stage note)"
                + " (guest workload failed: action=Audio error=System.IO.DirectoryNotFoundException)")
        }
    }

    func testPresentOutputPasses() throws {
        try Data("ok\n".utf8).write(to: share.appendingPathComponent("t17-audio-0123456789ab.txt"))
        try T17GuestWorkloadOutput.await(share: share, name: "t17-audio-0123456789ab.txt", prefix: "0123456789ab", timeout: 1)
    }
}
