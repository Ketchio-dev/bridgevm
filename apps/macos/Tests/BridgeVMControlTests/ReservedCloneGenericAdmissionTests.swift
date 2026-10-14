import XCTest
@testable import BridgeVMControl

@MainActor
final class ReservedCloneGenericAdmissionTests: XCTestCase {
    func testAcceptedCloneRefusesSharedFastVZStart() async throws {
        try await checkStart(kind: "fast-vz", shared: true)
    }

    func testAcceptedCloneRefusesSharedQemuAliasStart() async throws {
        try await checkStart(kind: "qemu-compat", shared: true, alias: true)
    }

    func testAcceptedClonePermitsDistinctFastVZStart() async throws {
        try await checkStart(kind: "fast-vz", shared: false)
    }

    func testAcceptedCloneRefusesSharedFastVZResourceRestart() async throws {
        try await checkResources(shared: true)
    }

    func testAcceptedClonePermitsDistinctFastVZResourceRestart() async throws {
        try await checkResources(shared: false)
    }

    private func checkStart(kind: String, shared: Bool, alias: Bool = false) async throws {
        let f = try ReservedCloneGenericFixture(kind: kind, shared: shared, alias: alias)
        defer { f.clean() }
        f.admitClone()
        f.model.start()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.calls[0], shared ? 0 : 1, "A reserved clone must prevent starting a shared writer")
        XCTAssertEqual(f.library.operationError != nil, shared)
        try f.assertUnchanged()
    }

    private func checkResources(shared: Bool) async throws {
        let f = try ReservedCloneGenericFixture(kind: "fast-vz", shared: shared)
        defer { f.clean() }
        f.admitClone()
        // Synthetic observation appears after clone admission. No process or guest runs.
        f.backend.setExternalRunning(true)
        f.model.pendingMemGiB = 4
        f.model.pendingCPU = 2
        f.model.applyResources()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.didBeginResourceUpdate, !shared)
        XCTAssertEqual(f.backend.calls[3], shared ? 0 : 1)
        XCTAssertEqual(f.backend.calls[0], shared ? 0 : 1)
        XCTAssertEqual(f.library.operationError != nil, shared)
        try f.assertUnchanged()
    }
}

