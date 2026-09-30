import XCTest
@testable import BridgeVMProductE2E

final class T17InputChallengeShareTests: XCTestCase {
    private let nonce = String(repeating: "7a", count: 32)
    private var share: URL!
    private var progress: URL { share.appendingPathComponent("t17-keyboard-pointer-progress-7a7a7a7a7a7a.txt") }

    override func setUpWithError() throws {
        share = FileManager.default.temporaryDirectory.appendingPathComponent("t17-share-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: share, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: share) }

    func testOnlyAnExactProgressLineIsReported() throws {
        let cases: [(String, String?)] = [
            ("clicked=1 typed=18\n", "clicked=1 typed=18"), ("clicked=0 typed=0\n", "clicked=0 typed=0"),
            ("clicked=1 typed=18", nil), ("clicked=2 typed=18\n", nil), ("clicked=1 typed=1000\n", nil),
            ("clicked=1 typed=18\nextra\n", nil), ("clicked=1 typed=18 \n", nil), ("\u{1b}[31mclicked=1 typed=1\n", nil),
        ]
        for (body, expected) in cases {
            try Data(body.utf8).write(to: progress)
            XCTAssertEqual(T17InputChallengeShare.progress(progress), expected, body.debugDescription)
        }
        try Data(String(repeating: "x", count: 64).utf8).write(to: progress)
        XCTAssertNil(T17InputChallengeShare.progress(progress))
    }

    func testSymlinkedProgressIsNotReported() throws {
        let target = share.appendingPathComponent("elsewhere")
        try Data("clicked=1 typed=18\n".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: progress, withDestinationURL: target)
        XCTAssertNil(T17InputChallengeShare.progress(progress))
    }

    func testMissingOutputSaysWhatTheFormSaw() throws {
        XCTAssertThrowsError(try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: 0.3)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest workload did not produce t17-keyboard-pointer-7a7a7a7a7a7a.txt")
        }
        try Data("clicked=1 typed=0\n".utf8).write(to: progress)
        XCTAssertThrowsError(try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: 0.3)) {
            XCTAssertEqual($0 as? T17Blocker, T17Blocker(code: "guest-evidence-missing", detail:
                "guest workload did not produce t17-keyboard-pointer-7a7a7a7a7a7a.txt (guest form saw clicked=1 typed=0)"))
        }
    }

    func testEmptyOutputIsUnsafe() throws {
        try Data().write(to: share.appendingPathComponent("t17-keyboard-pointer-7a7a7a7a7a7a.txt"))
        XCTAssertThrowsError(try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: 0.3)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest output is unsafe or oversized")
        }
    }
}
