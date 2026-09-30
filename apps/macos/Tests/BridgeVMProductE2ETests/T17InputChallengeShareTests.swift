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
        let good = "clicked=1 typed=18 session=1 integrity=high foreground=self cursor=400x300"
        let low = "clicked=0 typed=0 session=0 integrity=unknown foreground=other cursor=0x0"
        let cases: [(String, String?)] = [(good + "\n", good), (low + "\n", low),
            (good, nil), ("clicked=1 typed=18\n", nil), (good.replacingOccurrences(of: "high", with: "root") + "\n", nil),
            (good.replacingOccurrences(of: "typed=18", with: "typed=1000") + "\n", nil), (good + "\nextra\n", nil),
            (good + " \n", nil), ("\u{1b}[31m" + good + "\n", nil), (good.replacingOccurrences(of: "400x300", with: "-1x300") + "\n", nil),
        ]
        for (body, expected) in cases {
            try Data(body.utf8).write(to: progress)
            XCTAssertEqual(T17InputChallengeShare.progress(progress), expected, body.debugDescription)
        }
        try Data(String(repeating: "x", count: 200).utf8).write(to: progress); XCTAssertNil(T17InputChallengeShare.progress(progress))
    }

    func testSymlinkedProgressIsNotReported() throws {
        let target = share.appendingPathComponent("elsewhere")
        try Data("clicked=1 typed=18 session=1 integrity=high foreground=self cursor=400x300\n".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: progress, withDestinationURL: target)
        XCTAssertNil(T17InputChallengeShare.progress(progress))
    }

    func testMissingOutputSaysWhatTheFormSaw() throws {
        XCTAssertThrowsError(try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: 0.3)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest workload did not produce t17-keyboard-pointer-7a7a7a7a7a7a.txt")
        }
        let seen = "clicked=1 typed=0 session=1 integrity=medium foreground=other cursor=400x300"; try Data((seen + "\n").utf8).write(to: progress)
        XCTAssertThrowsError(try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: 0.3)) {
            XCTAssertEqual($0 as? T17Blocker, T17Blocker(code: "guest-evidence-missing", detail:
                "guest workload did not produce t17-keyboard-pointer-7a7a7a7a7a7a.txt (guest form saw \(seen))"))
        }
    }

    func testEmptyOutputIsUnsafe() throws {
        try Data().write(to: share.appendingPathComponent("t17-keyboard-pointer-7a7a7a7a7a7a.txt"))
        XCTAssertThrowsError(try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: 0.3)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest output is unsafe or oversized")
        }
    }
}
