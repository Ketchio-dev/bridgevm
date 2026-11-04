import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfAgentUpdateTests: XCTestCase {
    private func libraryConfig(_ bundle: String = "/vms/win.bvm") throws -> HvfEngineConfig {
        try XCTUnwrap(HvfEngineConfig.libraryVM(VMConfig(
            id: "vm", name: "vm", displayName: "vm", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle, runnerPath: "",
            launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "vm", displayWidth: 1280, displayHeight: 800,
            installPending: false)))
    }

    private func runner(_ config: HvfEngineConfig, assets: String?) -> [String] {
        config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p",
                               ownedRuntime: true, agentAssetsDir: assets)
    }

    func testLibraryVMPassesTheAppAgentTheBundleMarkerAndTheSession() throws {
        let arguments = runner(try libraryConfig(), assets: "/app/Resources/scripts/win-assets")
        let start = try XCTUnwrap(arguments.firstIndex(of: "--helper-agent-update"))
        XCTAssertEqual(Array(arguments[start..<start + 6]), [
            "--helper-agent-update", "/app/Resources/scripts/win-assets",
            "--helper-agent-update-marker", "/vms/win.bvm/metadata/agent-update.json",
            "--helper-agent-update-session", HvfAgentUpdate.session])
        XCTAssertEqual(HvfAgentUpdate.session, HvfAgentUpdate.session, "one ID for the whole app session")
        XCTAssertFalse(runner(try libraryConfig(), assets: nil).contains("--helper-agent-update"))
        var adHoc = try libraryConfig()
        adHoc.libraryContext = nil
        XCTAssertFalse(runner(adHoc, assets: "/app").contains("--helper-agent-update"), "only library VMs update")
    }

    func testEachResultIsOneKoreanLine() {
        let ok = HvfAgentUpdate.notice(for: "BVAGENT UPDATE ok from=none to=0123456789abcdef t=1")
        let failed = HvfAgentUpdate.notice(for: "BVAGENT UPDATE failed reason=arm detail=parse t=1")
        let late = HvfAgentUpdate.notice(for: "BVAGENT UPDATE failed reason=no-ready t=1")
        let rolledBack = HvfAgentUpdate.notice(for: "BVAGENT UPDATE rolled-back to=none t=1")
        XCTAssertEqual(ok, "게스트 도구를 새 버전으로 업데이트했습니다.")
        XCTAssertEqual(failed, "게스트 도구 업데이트에 실패해 이전 버전을 계속 사용합니다. 앱을 다시 시작하면 다시 시도합니다.")
        XCTAssertEqual(late, failed)
        XCTAssertEqual(rolledBack, "새 게스트 도구가 시작되지 않아 이전 버전으로 되돌렸습니다. 앱을 다시 시작하면 다시 시도합니다.")
        for text in [ok, failed, rolledBack] { XCTAssertFalse(text?.contains("\n") ?? true) }
        for quiet in ["BVAGENT UPDATE begin from=none to=0123456789abcdef t=1", "BVAGENT UPDATE armed to=0123456789abcdef t=2",
                      "BVAGENT UPDATE current agent=0123456789abcdef t=3", "BVAGENT UPDATE skipped reason=stop t=4",
                      "BVAGENT UPDATE confirm-failed detail= t=5", "BVAGENT UPDATE staged files=9 bytes=1 t=6",
                      "BVAGENT UPDATEok", "BVAGENT SHARE root-state ready"] {
            XCTAssertNil(HvfAgentUpdate.notice(for: quiet), quiet)
        }
    }

    func testTheSessionShowsTheLatestResultUntilItResets() {
        let notice = HvfAgentUpdateNotice()
        for event in BvAgentEvent.parse(lines: ["BVAGENT UPDATE begin from=none to=0123456789abcdef t=1",
                                                "BVAGENT UPDATE failed reason=no-ready t=2\r"]) {
            notice.consume(event)
        }
        XCTAssertEqual(notice.text, HvfAgentUpdate.notice(for: "BVAGENT UPDATE failed reason=no-ready t=2"))
        for event in BvAgentEvent.parse(lines: ["BVAGENT UPDATE ok from=earlier to=0123456789abcdef t=3"]) {
            notice.consume(event)
        }
        XCTAssertEqual(notice.text, "게스트 도구를 새 버전으로 업데이트했습니다.")
        notice.reset()
        XCTAssertNil(notice.text)
    }
}
