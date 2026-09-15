import Foundation
import Darwin
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeOutcomeTests: XCTestCase {
    func testRequireNewRefusesExternalRuntimeBeforePreparationOrLaunch() throws {
        let f = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { f.clean() }
        f.existingRuntime = true
        let session = f.session()
        let before = try FileManager.default.subpathsOfDirectory(atPath: f.root.path)
        guard case .refused = session.start(policy: .requireNew) else { return XCTFail("Expected refusal") }
        XCTAssertEqual(try FileManager.default.subpathsOfDirectory(atPath: f.root.path), before)
        XCTAssertEqual(f.lookups, 1)
        XCTAssertEqual(f.launches, 0)
        XCTAssertEqual(f.keys.requests, 0)
        XCTAssertNil(session.ownedProcessIdentity)
        XCTAssertEqual(session.connectionState, .stopped)
    }

    func testDefaultStartAttachesButOwnedStopCannotControlThatAttachment() throws {
        let f = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { f.clean() }
        f.existingRuntime = true
        let session = f.session()
        XCTAssertEqual(session.start(), .observedAttachment)
        let probes = f.lookups
        XCTAssertEqual(session.stopOwned(expectedToken: UUID()), .notOwned)
        XCTAssertEqual(f.lookups, probes)
        XCTAssertEqual(f.launches, 0)
        XCTAssertEqual(session.connectionState, .booting)
        XCTAssertNil(session.lastOwnedExit)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.config.ctlFilePath))
    }

    func testAdmissionAndReadinessRefuseBeforeAnyChildOrKeyAccess() throws {
        let f = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { f.clean() }
        let session = f.session()
        session.workAdmission = { _ in "reserved" }
        XCTAssertEqual(session.start(policy: .requireNew), .refused("reserved"))
        XCTAssertEqual(f.lookups, 0)
        session.workAdmission = nil
        guard case .failed(.readiness, _) = session.start(policy: .requireNew) else {
            return XCTFail("Readiness failure must be typed")
        }
        XCTAssertEqual(f.launches, 0)
        XCTAssertEqual(f.keys.requests, 0)
        XCTAssertNil(session.lastOwnedExit)
    }

    func testActualExitCodeIsPreservedAcrossStoppedStateAndNewLaunchIdentity() async throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        let session = f.session()
        guard case let .ownedLaunchAccepted(first) = session.start(policy: .requireNew) else {
            return XCTFail("Expected owned helper launch")
        }
        try await f.observeExit(session)
        let receipt = try XCTUnwrap(session.lastOwnedExit)
        XCTAssertEqual(receipt.identity, first)
        XCTAssertEqual(receipt.reason, .exit)
        XCTAssertEqual(receipt.status, 17)
        XCTAssertNil(session.ownedProcessIdentity)
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertEqual(session.stopOwned(expectedToken: first.token), .alreadyStopped)
        guard case let .ownedLaunchAccepted(second) = session.start(policy: .requireNew) else {
            return XCTFail("Expected a new accepted process")
        }
        XCTAssertNotEqual(first.token, second.token)
        XCTAssertEqual(session.stopOwned(expectedToken: first.token), .notOwned)
        try await f.observeExit(session)
        XCTAssertEqual(session.lastOwnedExit?.identity, second)
        XCTAssertEqual(f.launches, 2)
    }

    func testOwnedStopIsIdempotentAndWrongTokenDoesNotProbeOrStop() async throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        f.waitScript()
        let session = f.session()
        guard case let .ownedLaunchAccepted(identity) = session.start(policy: .requireNew) else {
            return XCTFail("Expected owned helper launch")
        }
        let probes = f.lookups
        XCTAssertEqual(session.stopOwned(expectedToken: UUID()), .notOwned)
        XCTAssertTrue(f.child?.isRunning == true)
        guard case let .requested(deadline) = session.stopOwned(expectedToken: identity.token) else {
            return XCTFail("Expected a first stop request")
        }
        XCTAssertNotNil(deadline)
        try await Task.sleep(nanoseconds: 30_000_000)
        XCTAssertEqual(session.stopOwned(expectedToken: identity.token), .alreadyStopping(deadline: deadline))
        session.stop()
        XCTAssertEqual(session.stopOwned(expectedToken: identity.token), .alreadyStopping(deadline: deadline))
        XCTAssertEqual(f.lookups, probes)
        XCTAssertNil(session.lastOwnedExit)
        try f.releaseChild()
        try await f.observeExit(session)
        XCTAssertEqual(session.lastOwnedExit?.status, 0)
    }

    func testActualSignalTerminationIsNotReportedAsSuccessfulExit() async throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        f.script = "exec /bin/sleep 10"
        let session = f.session()
        guard case .ownedLaunchAccepted = session.start(policy: .requireNew) else {
            return XCTFail("Expected owned helper launch")
        }
        try XCTUnwrap(f.child).terminate()
        try await f.observeExit(session)
        XCTAssertEqual(session.lastOwnedExit?.reason, .uncaughtSignal)
        XCTAssertEqual(session.lastOwnedExit?.status, SIGTERM)
    }

    func testKeyDeliveryFailureKeepsOwnedProcessBusyUntilObservedExit() async throws {
        let f = try HvfOwnedRuntimeFixture(encrypted: true)
        defer { f.clean() }
        f.waitScript(exit: 23, ignoringTerm: true)
        f.failKeyDelivery = true
        let session = f.session()
        guard case .failed(.keyDelivery, _) = session.start(policy: .requireNew) else {
            return XCTFail("A closed inherited key pipe must fail the real delivery")
        }
        let identity = try XCTUnwrap(session.ownedProcessIdentity)
        XCTAssertTrue(f.child?.isRunning == true)
        XCTAssertEqual(session.connectionState, .stopping)
        XCTAssertNil(session.lastOwnedExit)
        XCTAssertFalse(session.acceptStartConfiguration(f.config))
        guard case .refused = session.start(policy: .requireNew) else { return XCTFail("Still owned") }
        XCTAssertEqual(session.stopOwned(expectedToken: identity.token), .alreadyStopping(deadline: nil))
        XCTAssertEqual(f.launches, 1)
        XCTAssertEqual(f.keys.requests, 1)
        try f.releaseChild()
        try await f.observeExit(session)
        XCTAssertEqual(session.lastOwnedExit?.identity, identity)
        XCTAssertEqual(session.lastOwnedExit?.reason, .exit)
        XCTAssertEqual(session.lastOwnedExit?.status, 23)
    }

    func testLaunchFailureDoesNotInventAnOwnedProcessOrExitReceipt() throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        f.failLaunch = true
        let session = f.session()
        guard case .failed(.processLaunch, _) = session.start(policy: .requireNew) else {
            return XCTFail("Expected the actual launcher refusal")
        }
        XCTAssertNil(session.ownedProcessIdentity)
        XCTAssertNil(session.lastOwnedExit)
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertEqual(f.launches, 1)
    }
}
