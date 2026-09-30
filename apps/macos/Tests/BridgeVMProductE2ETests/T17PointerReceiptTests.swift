import XCTest
@testable import BridgeVMProductE2E

/// Writes the host transcript of one inserted POINTERINPUT request.
enum T17PointerReceiptFixture {
    static func receipt(_ id: String = UUID().uuidString, exit: Int = 0, events: Int = 1) -> String {
        "BVAGENT CMD POINTERINPUT \(id) exit=\(exit)\nBVINPUT_INSERTED \(id) \(events)\nBVAGENT END POINTERINPUT \(id)\n"
    }

    static func click(_ runLog: URL) throws {
        if !FileManager.default.fileExists(atPath: runLog.path) { try Data().write(to: runLog) }
        let handle = try FileHandle(forWritingTo: runLog); defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((receipt() + receipt()).utf8))
    }
}

final class T17PointerReceiptTests: XCTestCase {
    func testCountsOnlyCompleteSuccessfulPointerReceipts() {
        let id = UUID().uuidString
        XCTAssertEqual(T17PointerReceipt.inserted(in: T17PointerReceiptFixture.receipt() + T17PointerReceiptFixture.receipt()), 2)
        XCTAssertEqual(T17PointerReceipt.inserted(in: T17PointerReceiptFixture.receipt().replacingOccurrences(of: "\n", with: "\r\n")), 1)
        XCTAssertEqual(T17PointerReceipt.inserted(in: T17PointerReceiptFixture.receipt(exit: 1)), 0)
        XCTAssertEqual(T17PointerReceipt.inserted(in: T17PointerReceiptFixture.receipt(events: 0)), 0)
        XCTAssertEqual(T17PointerReceipt.inserted(in: "BVAGENT CMD TEXTINPUT \(id) exit=0\nBVINPUT_INSERTED \(id) 36\nBVAGENT END TEXTINPUT \(id)\n"), 0)
        XCTAssertEqual(T17PointerReceipt.inserted(in: "BVAGENT CMD POINTERINPUT \(id) exit=0\nBVINPUT_INSERTED \(UUID().uuidString) 1\nBVAGENT END POINTERINPUT \(id)\n"), 0)
        XCTAssertEqual(T17PointerReceipt.inserted(in: "BVAGENT CMD POINTERINPUT \(id) exit=0\nBVINPUT_INSERTED \(id) 1\n"), 0)
    }

    func testClickPassesOnlyWithReceiptsWrittenAfterIt() throws {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("t17-pointer-\(UUID().uuidString).log")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data((T17PointerReceiptFixture.receipt() + T17PointerReceiptFixture.receipt()).utf8).write(to: log)
        XCTAssertThrowsError(try T17PointerReceipt.require(log, timeout: 0.5) {}) {
            XCTAssertEqual($0 as? T17Blocker, T17Blocker(code: "guest-evidence-missing",
                                                         detail: "display click was not inserted as guest pointer input"))
        }
        try T17PointerReceipt.require(log, timeout: 5) {
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { try? T17PointerReceiptFixture.click(log) }
        }
    }

    func testOneInsertionIsNotAClick() throws {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("t17-pointer-\(UUID().uuidString).log")
        defer { try? FileManager.default.removeItem(at: log) }
        XCTAssertThrowsError(try T17PointerReceipt.require(log, timeout: 0.5) {
            try Data(T17PointerReceiptFixture.receipt().utf8).write(to: log)
        })
    }
}
