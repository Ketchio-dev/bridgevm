import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfGUIStartLibraryTests: XCTestCase {
    private func config(_ root: URL) -> VMConfig {
        VMConfig(id: "gui-retained", name: "gui-retained", displayName: "GUI retained", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("gui-retained/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "fixture", displayWidth: 1280, displayHeight: 720, installPending: false)
    }
    func testRemovedRegistrationKeepsPendingSessionAndRefusesNewEffectsAfterWorkerReturns() async throws {
        let gate = HvfGUIStartGate()
        let f = try HvfGUIStartFixture(encrypted: true, probeGate: gate)
        do {
            let root = f.root.appendingPathComponent("library"), saved = config(f.root.appendingPathComponent("library"))
            XCTAssertTrue(VMLibrary.save(saved, rootURL: root))
            var factories = 0
            let model = LibraryModel(rootURL: root, migrateLegacy: false,
                runtimeSessionFactory: { _ in factories += 1; return f.session }, startsModelsAutomatically: false)
            XCTAssertTrue(model.hvfRuntimeSession(for: saved) === f.session)
            var edited = f.base.config; edited.ramMiB = 2048
            let registration = root.appendingPathComponent(saved.slug + "/vm.json")
            let original = try Data(contentsOf: registration)
            let ticket = try f.start(configuration: edited)
            try await f.wait { gate.state.entered }
            XCTAssertEqual(try Data(contentsOf: registration), original, "GUI edits do not rewrite saved CLI identity")
            try FileManager.default.removeItem(at: registration); model.reload()
            XCTAssertTrue(model.hvfRuntimeSessions.existingRecord(slug: saved.slug)?.session === f.session)
            let retained = try XCTUnwrap(model.retainedControlRecords.first { record in
                if case let .runtime(_, session) = record.descriptor { return session === f.session }; return false
            })
            XCTAssertTrue(retained.descriptor.isActive); XCTAssertFalse(model.dismissRetainedControl(retained.id))
            XCTAssertEqual(factories, 1); XCTAssertTrue(f.session.hasPendingGUIStart)
            XCTAssertNil(f.session.reserveRuntimeMutation(kind: .snapshot))
            gate.release(); try await f.wait { !ticket.workerPending }
            if case .refused = ticket.outcome {} else { XCTFail("Removed registration must invalidate the worker") }
            XCTAssertEqual(f.effects.snapshot.launches, 0); XCTAssertTrue(f.effects.snapshot.keyCreation.isEmpty)
            XCTAssertFalse(FileManager.default.fileExists(atPath: edited.evidenceDir))
            XCTAssertFalse(f.session.hasActiveRuntimeWork)
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testPendingGUIBlocksCLISavedStartBeforeRegistrationOrAdditionalFactory() async throws {
        let gate = HvfGUIStartGate()
        let f = try HvfGUIStartFixture(rejectLaunch: true, probeGate: gate)
        do {
            let root = f.root.appendingPathComponent("library"), saved = config(f.root.appendingPathComponent("library"))
            XCTAssertTrue(VMLibrary.save(saved, rootURL: root)); var factories = 0
            let model = LibraryModel(rootURL: root, migrateLegacy: false,
                runtimeSessionFactory: { _ in factories += 1; return f.session }, startsModelsAutomatically: false)
            XCTAssertTrue(model.hvfRuntimeSession(for: saved) === f.session)
            let ticket = try f.start(); try await f.wait { gate.state.entered }
            let identity = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false).identity
            let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .start,
                requestID: UUID().uuidString, library: identity, vmID: saved.slug, appInstanceID: UUID().uuidString,
                expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: saved), operationID: UUID().uuidString)
            XCTAssertThrowsError(try model.requestOwnedRuntimeStart(request, validateOwner: {}, onReserved: { _ in XCTFail("CLI overlaps pending GUI") })) {
                XCTAssertEqual($0 as? NativeRuntimeStartRefusal, .busy)
            }
            XCTAssertEqual(factories, 1); XCTAssertEqual(f.effects.snapshot.launches, 0)
            gate.release(); try await f.wait { !ticket.workerPending }
        } catch { await f.clean(); throw error }
        await f.clean()
    }
}
