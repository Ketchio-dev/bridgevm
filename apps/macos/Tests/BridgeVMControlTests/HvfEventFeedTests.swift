import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfEventFeedTests: XCTestCase {
    @MainActor
    private func session() -> HvfEngineSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        return HvfEngineSession(config: config, repoRoot: root) { _ in true }
    }

    @MainActor
    func testTrimmingKeepsEachRemainingRowsIdentityAndContent() {
        let s = session()
        for i in 0..<600 { s.append(.unknown("line \(i)")) }
        let before = Dictionary(uniqueKeysWithValues: s.eventFeed.map { ($0.id, $0.event.displayText) })
        XCTAssertEqual(s.eventFeed.first?.id, 100); XCTAssertEqual(s.eventFeed.last?.id, 599); XCTAssertEqual(s.eventFeed.count, 500)
        s.append(.unknown("line 600"))
        for row in s.eventFeed where row.id < 600 { XCTAssertEqual(before[row.id], row.event.displayText) }
        XCTAssertEqual(s.eventFeed.first?.id, 101); XCTAssertEqual(s.eventFeed.last?.id, 600)
    }

    @MainActor
    func testClearingNeverReusesAnId() {
        let s = session()
        for i in 0..<3 { s.append(.unknown("a\(i)")) }
        s.resetObservedRuntimeState(clearEvents: true)
        s.append(.unknown("b"))
        XCTAssertEqual(s.eventFeed.map(\.id), [3])
    }
}
