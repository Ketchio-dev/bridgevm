import CoreGraphics
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfSavedDisplayRestoreTests: XCTestCase {
    func testSavedResolutionReturnsAfterRestartWithoutOpeningADisplayWindow() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.config.guestResolution = "2560x1600"
            session.config.virtioGpu2d = true
            session.beginOwnedInputBoot()
            XCTAssertNil(session.displayResize.view)
            try append("BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n", fixture)
            session.poll()
            let resize = "RESIZE 2560x1600\n"
            XCTAssertEqual(try fixture.bytes(fixture.input), Data(resize.utf8))
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data(resize.utf8), "one request per boot")

            try append(HvfGuestResetRecord.processRecreation + "\nBVAGENT READY host=test t=3\n", fixture)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data(resize.utf8), "wait for the recreated helper's service")
            try append("BVAGENT SERVICE start t=4\n", fixture)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data((resize + resize).utf8))
            XCTAssertNil(session.displayResize.view, "restoration opens no display window")
        }
    }

    func testALaidOutRetinaWindowOverridesTheSavedBootResolution() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.config.guestResolution = "2560x1600"
            session.config.virtioGpu2d = true
            session.config.retinaResolution = true
            session.beginOwnedInputBoot()
            try append("BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n", fixture)
            session.poll()
            session.displayViewResized(to: CGSize(width: 1517, height: 931), scale: 2, settled: true)
            let before = "RESIZE 2560x1600\nRESIZE 3034x1862@200\n"
            XCTAssertEqual(try fixture.bytes(fixture.input), Data(before.utf8))
            try append(HvfGuestResetRecord.processRecreation + "\nBVAGENT READY host=test t=3\nBVAGENT SERVICE start t=4\n", fixture)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data((before + "RESIZE 3034x1862@200\n").utf8))
        }
    }

    private func append(_ text: String, _ fixture: HvfRuntimeKeyboardDraftFixture) throws {
        let log = fixture.evidence.appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: log.path) { FileManager.default.createFile(atPath: log.path, contents: nil) }
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
