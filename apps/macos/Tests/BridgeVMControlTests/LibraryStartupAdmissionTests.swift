import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryStartupAdmissionTests: XCTestCase {
    func testLoadingRefusesCommandsWithoutReplayingThemWhenModelBecomesReady() async throws {
        let fixture = try LibraryStartupFixture(), probe = LibraryStartupProbe()
        defer { fixture.clean() }
        let startup = fixture.coordinator(probe: probe)
        let load = Task { await startup.load() }
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered)
        let app = UUID().uuidString, identity = fixture.library.identity
        let retained = { fixture.cache.retainedValue }
        let validate = { try fixture.owner.validateCurrentOwnership() }
        let starts = NativeRuntimeAppStartRouter(appInstanceID: app, library: identity, retainedModel: retained)
        let stops = NativeRuntimeAppControlRouter(appInstanceID: app, library: identity, retainedModel: retained)
        let installs = NativeRuntimeAppInstallRouter(appInstanceID: app, library: identity,
            validateOwner: validate, retainedModel: retained)
        func start() -> NativeRuntimeStartRequest {
            .init(schema: NativeRuntimeStartCodec.requestSchema, operation: .start, requestID: UUID().uuidString,
                library: identity, vmID: "missing", appInstanceID: app, operationID: UUID().uuidString)
        }
        let startRequest = start()
        let stopRequest = NativeRuntimeControlRequest(schema: NativeRuntimeControlCodec.requestSchema, operation: .stop,
            requestID: UUID().uuidString, library: identity, vmID: "missing", operationID: UUID().uuidString,
            target: .init(appInstanceID: app, runToken: UUID().uuidString, processID: 123))
        let installRequest = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema,
            operation: .installStatus, requestID: UUID().uuidString, library: identity, vmID: "missing",
            appInstanceID: app, operationID: nil)
        let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
            requestID: UUID().uuidString, library: identity, vmID: "missing",
            savedConfiguration: .init(state: .missing, digest: nil))
        let refusalWhileLoading = try starts.handle(startRequest).reason
        XCTAssertEqual(refusalWhileLoading, "Open the VM library in BridgeVM, then try again.")
        XCTAssertEqual(try stops.handle(stopRequest).reason, NativeRuntimeStopRefused.modelUnavailable.reason)
        XCTAssertEqual(try installs.handle(installRequest).refusal, .modelUnavailable)
        XCTAssertThrowsError(try NativeRuntimeAppObservation.response(statusRequest, library: identity,
            appInstanceID: app, retainedModel: retained())) { XCTAssertEqual($0 as? NativeRuntimeError, .snapshotUnavailable) }
        XCTAssertNil(fixture.cache.retainedValue); XCTAssertEqual(fixture.publications, 0)
        probe.release(); await load.value
        XCTAssertNotNil(startup.model); XCTAssertEqual(fixture.publications, 1)
        // Existing protocol semantics: a refused start ID stays refused; stop/install are re-evaluated.
        XCTAssertEqual(try starts.handle(startRequest).reason, refusalWhileLoading)
        XCTAssertEqual(try starts.handle(start()).reason?.contains("missing"), true)
        XCTAssertEqual(try stops.handle(stopRequest).reason, NativeRuntimeStopRefused.targetUnavailable.reason)
        XCTAssertEqual(try installs.handle(installRequest).refusal, .targetUnavailable)
        XCTAssertTrue(try NativeRuntimeAppObservation.response(statusRequest, library: identity,
            appInstanceID: app, retainedModel: retained()).sessions.isEmpty)
        XCTAssertTrue(startup.model?.cachedControlModels.isEmpty == true)
        XCTAssertFalse(probe.observations.timedOut)
    }
}
