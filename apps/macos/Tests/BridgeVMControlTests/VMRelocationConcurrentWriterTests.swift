import Foundation
import Darwin
import XCTest
@testable import BridgeVMControl

final class VMRelocationConcurrentWriterTests: XCTestCase {
    private final class Results: @unchecked Sendable {
        private let lock = NSLock()
        private var winners: [Int] = []
        private var errors: [String] = []
        func record(_ id: Int, error: Error?) {
            lock.lock()
            defer { lock.unlock() }
            if let error = error as NSError? {
                errors.append("\(error.domain):\(error.code)")
            } else {
                winners.append(id)
            }
        }
        func snapshot() -> ([Int], [String]) {
            lock.lock()
            defer { lock.unlock() }
            return (winners, errors)
        }
    }

    func testOnlyOneConcurrentWriterPublishesItsRecord() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("record")
        let results = Results()
        DispatchQueue.concurrentPerform(iterations: 16) { id in
            do {
                try VMRelocationRecordWriter.write(Data("writer-\(id)".utf8), to: file)
                results.record(id, error: nil)
            } catch {
                results.record(id, error: error)
            }
        }
        let (winners, errors) = results.snapshot()
        XCTAssertEqual(winners.count, 1)
        XCTAssertEqual(errors.count, 15)
        XCTAssertTrue(errors.allSatisfy { $0 == "\(NSPOSIXErrorDomain):\(EEXIST)" })
        let winner = try XCTUnwrap(winners.first)
        XCTAssertEqual(VMRelocationRecordReader.read(file), Data("writer-\(winner)".utf8))
    }
}
