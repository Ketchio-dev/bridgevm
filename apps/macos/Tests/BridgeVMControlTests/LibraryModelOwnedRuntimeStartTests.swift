import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryModelOwnedRuntimeStartTests: XCTestCase {
    func testOwnerLibraryAndTargetRefusalsPrecedeSessionCreation() throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config); let model = f.library()
        let request = try makeRequest(f.root, config: config)
        let before = try f.snapshot()
        XCTAssertThrowsError(try model.requestOwnedRuntimeStart(request,
            validateOwner: { throw NativeRuntimeError.libraryChanged }, onReserved: { _ in XCTFail("Reserved under invalid owner") })) {
            XCTAssertEqual($0 as? NativeRuntimeError, .libraryChanged)
        }
        let identity = request.library
        let wrong = NativeRuntimeLibraryIdentity(canonicalPath: identity.canonicalPath,
            device: identity.device, inode: identity.inode + 1, uid: identity.uid)
        assertRefused(.configurationChanged, model: model, request: try makeRequest(f.root, config: config, library: wrong))
        assertRefused(.targetUnavailable, model: model, request: try makeRequest(f.root, config: config, vmID: "missing"))
        model.vms = [config, config]
        assertRefused(.ambiguousTarget, model: model, request: request)
        XCTAssertEqual(f.runtimeCreations, 0); XCTAssertEqual(f.access.keyRequests, 0)
        XCTAssertEqual(try f.snapshot(), before)
    }

    func testStaleSavedDigestOrDiskModelMismatchRefusesWithoutEffects() throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config); let model = f.library()
        let request = try makeRequest(f.root, config: config)
        assertRefused(.configurationChanged, model: model,
            request: try makeRequest(f.root, config: config, digest: String(repeating: "0", count: 64)))
        var changed = config; changed.memMiB = 8192; f.save(changed)
        let changedBytes = try f.snapshot()
        assertRefused(.configurationChanged, model: model, request: request)
        model.reload()
        assertRefused(.configurationChanged, model: model, request: request)
        XCTAssertEqual(f.runtimeCreations, 0); XCTAssertEqual(f.access.processLookups, 0)
        XCTAssertEqual(try f.snapshot(), changedBytes)
    }

    func testPendingInstallAndOtherBackendRefuseBeforeRuntimeFactory() throws {
        for pending in [false, true] {
            let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
            var config = f.config(pending: pending)
            if !pending { config.backendKind = "fast-vz" }
            f.save(config); let model = f.library(); let before = try f.snapshot()
            assertRefused(.unsupportedRuntime, model: model, request: try makeRequest(f.root, config: config))
            XCTAssertEqual(f.runtimeCreations, 0); XCTAssertEqual(f.modelCreations, 0)
            XCTAssertEqual(f.access.keyRequests, 0); XCTAssertEqual(try f.snapshot(), before)
        }
    }

    func testRemovedButRetainedRuntimeBlocksReaddedRegistration() throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config); let model = f.library()
        let session = try f.runtime(config, in: model)
        session.connectionState = .timedOut
        try FileManager.default.removeItem(at: f.registration(config)); model.reload()
        XCTAssertTrue(model.retainedControlRecords.contains { record in
            if case let .runtime(_, retained) = record.descriptor { return retained === session }; return false
        })
        var replacement = config; replacement.memMiB = 8192
        f.save(replacement); model.reload()
        let creations = f.runtimeCreations, before = try f.snapshot()
        assertRefused(.busy, model: model, request: try makeRequest(f.root, config: replacement))
        XCTAssertEqual(f.runtimeCreations, creations); XCTAssertEqual(session.connectionState, .timedOut)
        XCTAssertEqual(f.access.processLookups, 0); XCTAssertEqual(f.access.keyRequests, 0)
        XCTAssertEqual(try f.snapshot(), before)
    }

    func testReservationPublicationFailureNeverSchedulesWorkerOrChangesFiles() throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config)
        var session: HvfEngineSession?
        let model = LibraryModel(rootURL: f.root, migrateLegacy: false, runtimeSessionFactory: { launch in
            let value = HvfEngineSession(config: launch, repoRoot: f.runtimeRepo,
                processIsRunning: { _ in XCTFail("Rejected admission performed a process lookup"); return false }, vtpmKeyProvider: f.access)
            value.ownedStartWorker = { _ in XCTFail("Rejected admission scheduled a worker"); return .failure(.preparation) }
            session = value; return value
        }, startsModelsAutomatically: false)
        let request = try makeRequest(f.root, config: config), before = try f.snapshot()
        var ticket: HvfOwnedStartOperation?
        let result = try model.requestOwnedRuntimeStart(request, validateOwner: {}, onReserved: { reserved in
            ticket = reserved
            XCTAssertTrue(session?.matchesRuntimeReservation(reserved.operationID) == true)
            throw NativeRuntimeError.timedOut
        })
        if case let .refused(reason) = result { XCTAssertEqual(reason, .admissionRefused) }
        else { XCTFail("Rejected publication was admitted") }
        XCTAssertEqual(ticket?.observation.phase, .failed); XCTAssertEqual(ticket?.workerPending, false)
        XCTAssertFalse(session?.hasActiveRuntimeWork == true)
        XCTAssertEqual(f.access.keyRequests, 0); XCTAssertEqual(try f.snapshot(), before)
    }

    func testReservationExclusionNeverHidesAnotherRetainedRuntime() throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config); let model = f.library()
        let session = try f.runtime(config, in: model)
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .snapshot))
        defer { session.finishRuntimeMutation(reservation) }
        XCTAssertTrue(session.checkReservedWork(reservation.id))
        XCTAssertNotNil(model.libraryWorkRefusal(slug: config.slug, excluding: (session, UUID()), owns: { _, current in current == config }))
        let other = f.makeRuntime(session.config); other.connectionState = .booting
        model.retainedControlStore.capture([.runtime(config: config, session: other)])
        XCTAssertFalse(session.checkReservedWork(reservation.id))
        XCTAssertFalse(session.validateRuntimeMutation(reservation))
        assertRefused(.busy, model: model, request: try makeRequest(f.root, config: config))
        XCTAssertEqual(f.access.processLookups, 0); XCTAssertEqual(f.access.keyRequests, 0)
    }

    private func makeRequest(_ root: URL, config: VMConfig, vmID: String? = nil,
                             library: NativeRuntimeLibraryIdentity? = nil, digest: String? = nil) throws -> NativeRuntimeStartRequest {
        let identity = try library ?? NativeRuntimeLibraryHandle.open(rootURL: root, create: false).identity
        return NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .start,
            requestID: UUID().uuidString, library: identity, vmID: vmID ?? config.slug,
            appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: try digest ?? NativeRuntimeConfigurationIdentity.digest(config: config),
            operationID: UUID().uuidString)
    }

    private func assertRefused(_ expected: NativeRuntimeStartRefusal, model: LibraryModel, request: NativeRuntimeStartRequest) {
        XCTAssertThrowsError(try model.requestOwnedRuntimeStart(request, validateOwner: {}, onReserved: { _ in XCTFail("Refused request reserved work") })) {
            XCTAssertEqual($0 as? NativeRuntimeStartRefusal, expected)
        }
    }
}
