import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeRetainedControlFixture {
    let work: HvfRuntimeWorkAdmissionFixture

    init(validator: HvfWindowsInstallValidationProbe = .init(error: "retained validation refusal", gateFirstCall: true)) throws {
        work = try HvfRuntimeWorkAdmissionFixture(validator: validator)
    }

    var effects: [Int] {
        [work.modelCreations, work.runtimeCreations, work.installCreations,
         work.access.processLookups, work.access.keyRequests, work.validator.snapshot.calls,
         work.queue.count, work.installJobCount]
    }

    func removeRegistration(_ config: VMConfig) throws {
        try FileManager.default.removeItem(at: work.registration(config))
    }

    // Inspect constructed values only; never evaluate child bodies or descend class graphs.
    func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    func runtimeRecord(_ session: HvfEngineSession, in library: LibraryModel) -> LibraryRetainedControlRecord? {
        library.retainedControlRecords.first { record in
            guard case let .runtime(_, candidate) = record.descriptor else { return false }
            return candidate === session
        }
    }

    func installRecord(_ session: HvfWindowsInstallSession, in library: LibraryModel) -> LibraryRetainedControlRecord? {
        library.retainedControlRecords.first { record in
            guard case let .install(_, candidate) = record.descriptor else { return false }
            return candidate === session
        }
    }

    func selectedView(in library: LibraryModel) -> RetainedControlDetailView? {
        let body = LibraryDetailView(library: library).body
        let found = values(RetainedControlDetailView.self, in: body)
        XCTAssertEqual(found.count, 1, "Actual library detail must route to an existing retained control")
        XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
        XCTAssertTrue(values(HvfWindowsInstallView.self, in: body).isEmpty)
        XCTAssertTrue(values(VMDetailPanel.self, in: body).isEmpty)
        XCTAssertTrue(values(FleetTableView.self, in: body).isEmpty)
        return found.first
    }

    @discardableResult
    func assertRuntimeRoute(_ session: HvfEngineSession, config: VMConfig,
                            in library: LibraryModel) -> String? {
        let before = effects
        guard let view = selectedView(in: library) else { return nil }
        guard case let .runtime(accepted, actual) = view.record.descriptor else {
            XCTFail("Retained runtime selection must keep its runtime kind"); return nil
        }
        XCTAssertTrue(actual === session)
        XCTAssertEqual(accepted, config)
        XCTAssertEqual(view.record.id, library.selectedID)
        XCTAssertEqual(library.selectedRetainedControl?.id, view.record.id)
        XCTAssertEqual(effects, before)
        return view.record.id
    }

    @discardableResult
    func assertInstallRoute(_ session: HvfWindowsInstallSession, config: VMConfig,
                            in library: LibraryModel) -> String? {
        let before = effects
        guard let view = selectedView(in: library) else { return nil }
        guard case let .install(accepted, actual) = view.record.descriptor else {
            XCTFail("Retained install selection must keep its install kind"); return nil
        }
        XCTAssertTrue(actual === session)
        XCTAssertEqual(accepted, config)
        XCTAssertEqual(view.record.id, library.selectedID)
        XCTAssertEqual(library.selectedRetainedControl?.id, view.record.id)
        XCTAssertEqual(effects, before)
        return view.record.id
    }

    /// Releases and acknowledges the finite worker before returning or rethrowing fixture errors.
    func withValidatingInstall(_ config: VMConfig, library: LibraryModel,
        during: @MainActor (HvfWindowsInstallSession) async throws -> Void) async throws -> HvfWindowsInstallSession {
        let session = work.install(config, in: library)
        guard let handle = session.start() else {
            XCTFail("Owned installer fixture must accept its first validation"); return session
        }
        do {
            let entered = await work.validator.waitForEntry()
            XCTAssertTrue(entered, "The finite owned validator must enter before removal")
            if entered { try await during(session) }
        } catch {
            work.validator.release()
            await handle.value
            XCTAssertFalse(work.validator.snapshot.gateTimedOut)
            throw error
        }
        work.validator.release()
        await handle.value
        XCTAssertFalse(work.validator.snapshot.gateTimedOut)
        return session
    }

    func clean() {
        XCTAssertEqual(work.access.processLookups, 0, "Retained controls must never attach or launch")
        XCTAssertEqual(work.access.keyRequests, 0)
        XCTAssertEqual(work.queue.count, 0, "C never admits a file-operation job")
        work.clean()
    }
}
