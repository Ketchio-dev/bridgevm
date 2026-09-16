import CryptoKit
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedStartCLITests: XCTestCase {
    func testPairedRustStartsAndNativeCLIStopsThroughActualOwner() async throws { try await exercise(pairedStart: true) }
    func testNativeCLIStartsAndPairedRustStopsThroughActualOwner() async throws { try await exercise(pairedStart: false) }

    private func exercise(pairedStart: Bool) async throws {
        let runner = try HvfOwnedStartRunnerFixture()
        let fixture: HvfOwnedStartCLIFixture
        do { fixture = try HvfOwnedStartCLIFixture(runner: runner) }
        catch { await runner.clean(); throw error }
        do {
            let (exit, started) = try await fixture.command("start", pairedRust: pairedStart)
            XCTAssertEqual(exit, 0, "\(started)")
            XCTAssertEqual(started["schema"] as? String, "bridgevm.app-start.v1")
            XCTAssertEqual(started["started"] as? Bool, true)
            XCTAssertEqual(started["appInstanceID"] as? String, fixture.appInstanceID)
            let digest = try NativeRuntimeConfigurationIdentity.digest(config: runner.config)
            XCTAssertEqual(started["expectedSavedConfigurationDigest"] as? String, digest)
            let operationID = try XCTUnwrap(started["operationID"] as? String)
            let observation = try XCTUnwrap(started["observation"] as? [String: Any])
            XCTAssertEqual(observation["phase"] as? String, "started")
            let target = try XCTUnwrap(observation["target"] as? [String: Any])
            let proof = try XCTUnwrap(observation["proof"] as? [String: Any])
            let token = try XCTUnwrap(target["token"] as? String)
            try await runner.awaitChildren()
            XCTAssertEqual(target["processID"] as? Int32, runner.effects.snapshot.child?.processIdentifier)
            XCTAssertEqual(proof["helperPID"] as? Int32, runner.childIDs["helper"])
            XCTAssertEqual(proof["helperGeneration"] as? Int, 0)
            let manifest = try Data(contentsOf: URL(fileURLWithPath: runner.mapped.evidenceDir).appendingPathComponent("launch-manifest.json"))
            XCTAssertEqual(proof["manifestSHA256"] as? String, SHA256.hash(data: manifest).map { String(format: "%02x", $0) }.joined())
            XCTAssertEqual(runner.effects.snapshot.launches, 1)
            XCTAssertEqual(runner.effects.snapshot.keyIDs, [runner.config.slug]); XCTAssertEqual(runner.effects.snapshot.creations, 0)
            XCTAssertEqual(runner.witnesses.count, 2)
            let retained = try await fixture.startStatus(operationID: operationID)
            XCTAssertEqual(retained.observation?.phase, .started); XCTAssertEqual(retained.observation?.target?.token, token)
            let (statusExit, status) = try await fixture.command("status", pairedRust: !pairedStart)
            XCTAssertEqual(statusExit, 0); XCTAssertEqual(status["schema"] as? String, NativeRuntimeCodec.responseSchema)
            let sessions = try XCTUnwrap(status["sessions"] as? [[String: Any]])
            XCTAssertEqual(sessions.count, 1); XCTAssertEqual(sessions.first?["ownership"] as? String, "owned")
            XCTAssertEqual(sessions.first?["acceptedConfigurationDigest"] as? String, digest)
            let (duplicateExit, duplicate) = try await fixture.command("start", pairedRust: !pairedStart)
            XCTAssertEqual(duplicateExit, 1); XCTAssertEqual(duplicate["started"] as? Bool, false)
            XCTAssertEqual(runner.effects.snapshot.launches, 1)
            let (stopExit, stopped) = try await fixture.command("stop", pairedRust: !pairedStart)
            XCTAssertEqual(stopExit, 0, "\(stopped)"); XCTAssertEqual(stopped["complete"] as? Bool, true)
            let stopTarget = try XCTUnwrap(stopped["target"] as? [String: Any])
            XCTAssertEqual(stopTarget["runToken"] as? String, token)
            let stopObservation = try XCTUnwrap(stopped["observation"] as? [String: Any])
            XCTAssertEqual(stopObservation["phase"] as? String, "completed"); XCTAssertNotNil(stopObservation["runnerExit"])
            let cleanup = try XCTUnwrap(stopObservation["supervisorComplete"] as? [String: Any])
            XCTAssertEqual(cleanup["helperReapedCount"] as? Int, 1); XCTAssertEqual(cleanup["swtpmReapedCount"] as? Int, 1)
            XCTAssertEqual(cleanup["mediaLeaseDisposition"] as? String, "releasedAfterReap")
            XCTAssertEqual(cleanup["runtimeDirectoryDisposition"] as? String, "removed")
            XCTAssertTrue(runner.witnesses.values.allSatisfy { $0.observe() })
            XCTAssertFalse(runner.effects.snapshot.child?.isRunning == true)
            XCTAssertFalse(runner.session?.mayHaveOwnedWork == true)
            let historical = try await fixture.startStatus(operationID: operationID)
            XCTAssertEqual(historical.observation?.phase, .started); XCTAssertEqual(historical.observation?.proof, retained.observation?.proof)
            await fixture.close()
        } catch { await fixture.close(); throw error }
    }
}
