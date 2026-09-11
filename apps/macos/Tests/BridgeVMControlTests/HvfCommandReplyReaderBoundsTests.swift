import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfCommandReplyReaderBoundsTests: XCTestCase {
    func testMalformedExitFieldsCannotReportSuccess() throws {
        try fixture { url in
            for value in ["0junk", "+0", "00", "--1", "2147483648", "0 "] {
                try response(code: value).write(to: url)
                XCTAssertNil(HvfCommandReplyReader(command: "ping", offset: 0).readReply(from: url), value)
            }
        }
    }

    func testCanonicalNegativeExitIsPreserved() throws {
        try fixture { url in
            try response(code: "-7").write(to: url)
            let reply = try XCTUnwrap(HvfCommandReplyReader(command: "ping", offset: 0).readReply(from: url))
            XCTAssertEqual(reply.code, -7)
            XCTAssertEqual(reply.output, "OK")
        }
    }

    func testOversizedPhysicalLineCannotForgeHeaderFromItsSuffix() throws {
        try fixture { url in
            var bytes = Data(repeating: 0x78, count: 300 * 1024)
            bytes.append(response())
            try bytes.write(to: url)
            let reader = HvfCommandReplyReader(command: "ping", offset: 0)
            XCTAssertNil(reader.readReply(from: url))
            try append(response(), to: url)
            XCTAssertEqual(reader.readReply(from: url)?.code, 0)
        }
    }

    func testSplitHeaderSurvivesTinyOutputLimit() throws {
        try fixture { url in
            let bytes = response()
            try Data(bytes.prefix(12)).write(to: url)
            let reader = HvfCommandReplyReader(command: "ping", offset: 0, outputLimitBytes: 1)
            XCTAssertNil(reader.readReply(from: url))
            try append(Data(bytes.dropFirst(12)), to: url)
            XCTAssertEqual(reader.readReply(from: url)?.code, 0)
        }
    }

    func testUnicodeOutputTailRespectsByteLimit() throws {
        try fixture { url in
            for body in [String(repeating: "\u{1f600}", count: 5), "a" + String(repeating: "\u{301}", count: 20)] {
                try response(body: body).write(to: url)
                let reader = HvfCommandReplyReader(command: "ping", offset: 0, outputLimitBytes: 5)
                let reply = try XCTUnwrap(reader.readReply(from: url))
                let retained = try XCTUnwrap(reply.output.split(separator: "\n", omittingEmptySubsequences: false).last)
                XCTAssertTrue(retained.utf8.count <= 5)
                XCTAssertFalse(retained.contains("\u{fffd}"))
            }
        }
    }

    func testPollingYieldsAfterOneMiB() throws {
        try fixture { url in
            var bytes = Data(repeating: 10, count: 1024 * 1024)
            bytes.append(response())
            try bytes.write(to: url)
            let reader = HvfCommandReplyReader(command: "ping", offset: 0)
            XCTAssertNil(reader.readReply(from: url))
            XCTAssertEqual(reader.readReply(from: url)?.output, "OK")
        }
    }

    func testFileTruncationResetsDiscardedPartialLine() throws {
        try fixture { url in
            try Data(repeating: 0x78, count: 300 * 1024).write(to: url)
            let reader = HvfCommandReplyReader(command: "ping", offset: 0)
            XCTAssertNil(reader.readReply(from: url))
            try response().write(to: url)
            XCTAssertEqual(reader.readReply(from: url)?.output, "OK")
        }
    }

    private func response(code: String = "0", body: String = "OK") -> Data {
        Data("BVAGENT CMD ping exit=\(code)\n\(body)\nBVAGENT END ping\n".utf8)
    }

    private func append(_ data: Data, to url: URL) throws {
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: data)
    }

    private func fixture(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root.appendingPathComponent("run.log"))
    }
}
