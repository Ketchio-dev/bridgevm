#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfClipboardPasteTargetTests: XCTestCase, HvfLegacyPasteFixture {
    func testTargetCancellationDropsDelayedLegacyPasteAndAllowsFreshRequest() throws {
        try withSession { session, root in
            XCTAssertEqual(session.sendText("한글"), .acceptedForProcessing)
            let old = try lastCommand(session)
            session.cancelOrderedInputTarget()
            try acknowledge(old, root: root)
            session.poll()
            let input = root.appendingPathComponent("input.ctl")
            XCTAssertFalse(FileManager.default.fileExists(atPath: input.path))
            XCTAssertEqual(session.sendText("새 입력"), .acceptedForProcessing)
            let fresh = try lastCommand(session)
            XCTAssertNotEqual(old, fresh)
            try acknowledge(old, root: root)
            session.poll()
            XCTAssertFalse(FileManager.default.fileExists(atPath: input.path))
            try acknowledge(fresh, root: root)
            session.poll()
            XCTAssertEqual(try String(contentsOf: input, encoding: .utf8), "KEY ctrl+v\n")
        }
    }

    func testDisplayTeardownAndSessionReplacementCancelLegacyPaste() throws {
        for replace in [false, true] {
            try withSession { session, root in
                let view = FBLayerView(session: session)
                defer { view.teardown() }
                XCTAssertEqual(session.sendText("이전 화면"), .acceptedForProcessing)
                let old = try lastCommand(session)
                if replace {
                    let next = HvfEngineSession(config: session.config, repoRoot: root) { _ in false }
                    view.updateSession(next)
                } else {
                    view.teardown()
                }
                try acknowledge(old, root: root)
                session.poll()
                XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            }
        }
    }
}
#endif
