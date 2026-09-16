import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeRunnerCLITests: XCTestCase {
    func testActualNativeCLIStopsOwnedRunAndRepeatsTheDurableOperation() async throws {
        let runner = try HvfOwnedRunnerFixture()
        let fixture: HvfOwnedRunnerCLIFixture
        do { fixture = try HvfOwnedRunnerCLIFixture(runner: runner) }
        catch { await runner.clean(); throw error }
        do {
            guard case let .ownedLaunchAccepted(identity) = runner.session.start(policy: .requireNew) else {
                throw CocoaError(.executableNotLoadable)
            }
            try await runner.awaitChildren()
            let (status, result) = try await fixture.stop(throughPairedRustCLI: true)
            XCTAssertEqual(status, 0)
            XCTAssertEqual(result["schema"] as? String, "bridgevm.app-stop.v1")
            XCTAssertEqual(result["complete"] as? Bool, true)
            let target = try XCTUnwrap(result["target"] as? [String: Any])
            XCTAssertEqual(target["appInstanceID"] as? String, fixture.appInstanceID)
            XCTAssertEqual(target["runToken"] as? String, identity.token.uuidString)
            XCTAssertEqual(target["processID"] as? Int32, identity.processID)
            let observation = try XCTUnwrap(result["observation"] as? [String: Any])
            XCTAssertEqual(observation["phase"] as? String, "completed")
            let cleanup = try XCTUnwrap(observation["supervisorComplete"] as? [String: Any])
            XCTAssertEqual(cleanup["helperReapedCount"] as? Int, 1)
            XCTAssertEqual(cleanup["swtpmReapedCount"] as? Int, 1)
            XCTAssertEqual(cleanup["mediaLeaseDisposition"] as? String, "releasedAfterReap")
            XCTAssertNotNil(observation["runnerExit"])
            let (repeatStatus, repeated) = try await fixture.stop()
            XCTAssertEqual(repeatStatus, 0)
            XCTAssertEqual(repeated["complete"] as? Bool, true)
            XCTAssertEqual(repeated["operationID"] as? String, result["operationID"] as? String)
            XCTAssertEqual(runner.base.launches, 1)
            XCTAssertFalse(runner.session.mayHaveOwnedWork)
            await fixture.close()
        } catch { await fixture.close(); throw error }
    }
}
