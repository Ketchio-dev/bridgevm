import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfGUIStartWorkerTests: XCTestCase {
    private func input(_ f: HvfGUIStartFixture, config: HvfEngineConfig? = nil,
                       policy: HvfRuntimeStartPolicy = .attachOrStart) -> HvfGUIStartExecution.Input {
        let effects = f.effects
        return .init(config: config ?? f.base.config, repoRoot: f.root, policy: policy,
            keyProvider: effects, processIsRunning: { effects.probe($0) }, launch: { try effects.launch($0) },
            effectAdmission: HvfRuntimeEffectAdmission(), permit: { true })
    }
    func testTypedAndLegacyRoutesReadInteractiveKeyOnceWithExistingStatePolicy() async throws {
        for typed in (_isDebugAssertConfiguration() ? [false, true] : [true]) {
            for populated in [false, true] {
                let f = try HvfGUIStartFixture(encrypted: true, typed: typed, rejectLaunch: true)
                do {
                    if populated {
                        let state = URL(fileURLWithPath: try XCTUnwrap(f.base.config.vtpmStateDir))
                        try FileManager.default.createDirectory(at: state, withIntermediateDirectories: true)
                        try Data([1]).write(to: state.appendingPathComponent("tpm2-00.permall"))
                    }
                    let request = input(f)
                    let output = await Task.detached { await HvfGUIStartWorker.run(request) }.value
                    guard case .failed(.processLaunch, _) = output.result else { throw CocoaError(.executableRuntimeMismatch) }
                    XCTAssertEqual(f.effects.snapshot.keyCreation, [!populated])
                    XCTAssertEqual(f.effects.snapshot.keyIDs, [try XCTUnwrap(f.base.config.vtpmKeyID)])
                    XCTAssertEqual(f.effects.snapshot.existingReads, 0); XCTAssertEqual(f.effects.snapshot.launches, 1)
                    XCTAssertTrue(f.effects.snapshot.offMain.allSatisfy { $0 })
                    XCTAssertEqual(f.effects.snapshot.executable, typed ? f.root.appendingPathComponent("target/release/hvf-runner").path : "/usr/bin/env")
                    XCTAssertNil(f.effects.snapshot.child)
                } catch { await f.clean(); throw error }
                await f.clean()
            }
        }
    }
    func testUnencryptedGUIStartDoesNotRequestAnyKey() async throws {
        let f = try HvfGUIStartFixture(rejectLaunch: true)
        let request = input(f)
        let output = await Task.detached { await HvfGUIStartWorker.run(request) }.value
        if case .failed(.processLaunch, _) = output.result {} else { XCTFail("Expected injected launch refusal") }
        XCTAssertTrue(f.effects.snapshot.keyCreation.isEmpty); XCTAssertEqual(f.effects.snapshot.existingReads, 0)
        XCTAssertEqual(f.effects.snapshot.launches, 1)
        await f.clean()
    }
    func testDefaultAttachmentAndRequireNewUseOneOffMainProbeWithoutOtherEffects() async throws {
        for policy in [HvfRuntimeStartPolicy.attachOrStart, .requireNew] {
            let f = try HvfGUIStartFixture(encrypted: true, foundExisting: true)
            let request = input(f, policy: policy)
            let output = await Task.detached { await HvfGUIStartWorker.run(request) }.value
            if policy == .attachOrStart {
                if case .observedAttachment = output.result {} else { XCTFail("Default policy must attach") }
            } else if case .refused = output.result {} else { XCTFail("requireNew cannot adopt") }
            XCTAssertEqual(f.effects.snapshot.probes, 1); XCTAssertEqual(f.effects.snapshot.launches, 0)
            XCTAssertTrue(f.effects.snapshot.keyCreation.isEmpty); XCTAssertEqual(f.effects.snapshot.offMain, [true])
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.base.config.evidenceDir))
            await f.clean()
        }
    }
    func testReadinessFailurePreservesAllDiagnosticsAndHasNoKeyOrChildEffects() async throws {
        let f = try HvfGUIStartFixture(encrypted: true)
        var config = f.base.config; config.targetDiskPath += ".missing"; config.ramMiB = 1
        let request = input(f, config: config)
        let output = await Task.detached { await HvfGUIStartWorker.run(request) }.value
        if case .failed(.readiness, _) = output.result {} else { XCTFail("Expected readiness failure") }
        XCTAssertTrue(output.diagnostics.contains { $0.contains("target-disk-missing") })
        XCTAssertTrue(output.diagnostics.contains { $0.contains("ram-range") })
        XCTAssertTrue(f.effects.snapshot.keyCreation.isEmpty); XCTAssertEqual(f.effects.snapshot.launches, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.evidenceDir))
        await f.clean()
    }
}
