import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfReleaseLegacyFallbackTests: XCTestCase {
    func testMissingRunnerRefusesExecutableWrapperBeforeKeyOrLaunch() async throws {
        try XCTSkipIf(_isDebugAssertConfiguration(), "Release-only fallback boundary")
        let fixture = try HvfGUIStartFixture()
        let effects = fixture.effects
        let input = HvfGUIStartExecution.Input(
            config: fixture.base.config, repoRoot: fixture.root, policy: .requireNew,
            keyProvider: effects, processIsRunning: { effects.probe($0) },
            launch: { try effects.launch($0) }, effectAdmission: HvfRuntimeEffectAdmission(),
            permit: { true })
        let gui = await Task.detached { await HvfGUIStartWorker.run(input) }.value
        if case .failed(.helper, _) = gui.result {} else { XCTFail("Release GUI worker must refuse wrapper fallback") }
        XCTAssertEqual(effects.snapshot.launches, 0)
        XCTAssertTrue(effects.snapshot.keyCreation.isEmpty)

        let session = fixture.session.start(policy: .requireNew)
        if case .failed(.helper, _) = session {} else { XCTFail("Release session must refuse wrapper fallback") }
        XCTAssertEqual(effects.snapshot.launches, 0)
        XCTAssertTrue(effects.snapshot.keyCreation.isEmpty)
        await fixture.clean()
    }
}
