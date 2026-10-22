import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ResourceRegistrationFailureTests: XCTestCase {
    func testCompensatedFailureReconcilesProvisionalRegistrationAndKeepsStop() async throws {
        let f = try ResourceRegistrationFixture(compensate: true)
        defer { f.backend.release() }
        _ = try await f.beginApply()
        try f.adoptOther()
        try await f.finishFailedApply()
        let restored = try NativeLibraryReader.readConfig(rootURL: f.libraryRoot, id: f.original.slug)
        XCTAssertEqual(restored, f.original)
        XCTAssertEqual(f.library.latestConfiguration(for: f.original), restored,
            "The failure boundary must replace the provisional allocation in the library")
        XCTAssertFalse(f.model.isRetained)
        XCTAssertTrue(f.library.retainedControlRecords.isEmpty)
        let controls = try f.ordinaryControls()
        XCTAssertTrue(controls.model === f.model)
        try await f.stopThroughDashboard(controls)
        XCTAssertEqual(f.backend.counts, [1, 0, 1])
        XCTAssertFalse(f.backend.isRunning())
        XCTAssertNil(f.library.operationError)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.libraryRoot, id: f.original.slug), restored)
    }

    func testActualWriterRefusalStillRetainsLaterExternalRegistrationChange() async throws {
        let f = try ResourceRegistrationFixture(validHandoff: false)
        defer { f.backend.release() }
        let before = try [f.registration, f.launch, f.handoff].map { try Data(contentsOf: $0) }
        _ = try await f.beginApply(expectSaved: false)
        try f.adoptOther()
        try await f.finishFailedApply()
        XCTAssertEqual(try [f.registration, f.launch, f.handoff].map { try Data(contentsOf: $0) }, before)
        XCTAssertFalse(f.model.isRetained)
        var external = f.original
        external.memMiB = 8192; external.cpuCount = 4
        XCTAssertTrue(VMLibrary.save(external, rootURL: f.libraryRoot))
        let bytes = try Data(contentsOf: f.registration)
        f.library.reload()
        XCTAssertTrue(f.model.isRetained, "Failed work must not exempt a later external allocation change")
        let record = try XCTUnwrap(f.library.retainedControlRecords.first)
        XCTAssertTrue(record.descriptor.isActive)
        f.model.stop()
        XCTAssertEqual(f.backend.counts, [1, 0, 0])
        XCTAssertEqual(try Data(contentsOf: f.registration), bytes)
    }
}
