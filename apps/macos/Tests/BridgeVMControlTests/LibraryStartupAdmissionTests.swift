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
        let digest = String(repeating: "a", count: 64)
        let retained = { fixture.cache.retainedValue }
        let validate = { try fixture.owner.validateCurrentOwnership() }
        let starts = NativeRuntimeAppStartRouter(appInstanceID: app, library: identity,
            validateOwner: validate, retainedModel: retained)
        let stops = NativeRuntimeAppControlRouter(appInstanceID: app, library: identity,
            validateOwner: validate, retainedModel: retained)
        let installs = NativeRuntimeAppInstallRouter(appInstanceID: app, library: identity,
            validateOwner: validate, retainedModel: retained)
        func start() -> NativeRuntimeStartRequest {
            .init(schema: NativeRuntimeStartCodec.requestSchema, operation: .start, requestID: UUID().uuidString,
                library: identity, vmID: "missing", appInstanceID: app,
                expectedSavedConfigurationDigest: digest, operationID: UUID().uuidString)
        }
        let startRequest = start()
        let stopRequest = NativeRuntimeControlRequest(schema: NativeRuntimeControlCodec.requestSchema, operation: .stop,
            requestID: UUID().uuidString, library: identity, vmID: "missing", operationID: UUID().uuidString,
            target: .init(appInstanceID: app, runToken: UUID().uuidString, processID: 123,
                acceptedConfigurationDigest: digest))
        let installRequest = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema,
            operation: .installStatus, requestID: UUID().uuidString, library: identity, vmID: "missing",
            appInstanceID: app, expectedSavedConfigurationDigest: digest, operationID: nil)
        let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
            requestID: UUID().uuidString, library: identity, vmID: "missing",
            savedConfiguration: .init(state: .missing, digest: nil))
        func context() -> NativeRuntimeRequestContext { .init(deadline: NativeRuntimeTransport.now + 5) }
        XCTAssertEqual(try starts.handle(startRequest, context: context()).refusal, .modelUnavailable)
        XCTAssertEqual(try stops.handle(stopRequest, context: context()).refusal, .modelUnavailable)
        XCTAssertEqual(try installs.handle(installRequest, context: context()).refusal, .modelUnavailable)
        XCTAssertThrowsError(try NativeRuntimeAppObservation.response(statusRequest, library: identity,
            appInstanceID: app, retainedModel: retained())) { XCTAssertEqual($0 as? NativeRuntimeError, .snapshotUnavailable) }
        XCTAssertNil(fixture.cache.retainedValue); XCTAssertEqual(fixture.publications, 0)
        probe.release(); await load.value
        XCTAssertNotNil(startup.model); XCTAssertEqual(fixture.publications, 1)
        // Existing protocol semantics: a refused start ID stays refused; stop/install are re-evaluated.
        XCTAssertEqual(try starts.handle(startRequest, context: context()).refusal, .modelUnavailable)
        XCTAssertEqual(try starts.handle(start(), context: context()).refusal, .targetUnavailable)
        XCTAssertEqual(try stops.handle(stopRequest, context: context()).refusal, .targetUnavailable)
        XCTAssertEqual(try installs.handle(installRequest, context: context()).refusal, .targetUnavailable)
        XCTAssertTrue(try NativeRuntimeAppObservation.response(statusRequest, library: identity,
            appInstanceID: app, retainedModel: retained()).sessions.isEmpty)
        XCTAssertTrue(startup.model?.cachedControlModels.isEmpty == true)
        XCTAssertFalse(probe.observations.timedOut)
    }
}
