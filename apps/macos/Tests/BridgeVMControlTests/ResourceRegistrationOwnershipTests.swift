import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ResourceRegistrationOwnershipTests: XCTestCase {
    func testOwnSuccessfulWriteSurvivesUnrelatedCreationAdoptionBeforeAcknowledgment() async throws {
        try await assertOrdinaryControl(interleaveAdoption: true)
    }

    func testOwnSuccessfulWriteWithoutInterleavingKeepsOrdinaryControl() async throws {
        try await assertOrdinaryControl(interleaveAdoption: false)
    }

    private func assertOrdinaryControl(interleaveAdoption: Bool) async throws {
        let f = try ResourceRegistrationFixture()
        defer { f.backend.release() }
        let saved = try await f.beginApply()
        let registrationBytes = try Data(contentsOf: f.registration), metadata = try f.metadataBytes()
        if interleaveAdoption { try f.adoptOther() }
        try await f.finishApply()
        XCTAssertEqual(f.model.config, saved)
        let controls = try f.ordinaryControls()
        XCTAssertTrue(controls.model === f.model, "Own resource publication must keep its existing active control")
        XCTAssertFalse(f.model.isRetained, "An accepted resource update is not an external registration replacement")
        XCTAssertTrue(f.library.retainedControlRecords.isEmpty)
        try await f.stopThroughDashboard(controls)
        XCTAssertEqual(f.backend.counts, [1, 1, 2], "Ordinary dashboard Stop must reach the inert backend")
        XCTAssertFalse(f.backend.isRunning(), "Stop must remain usable after the successful Apply")
        XCTAssertNil(f.library.operationError)
        XCTAssertEqual(try Data(contentsOf: f.registration), registrationBytes)
        XCTAssertEqual(try f.metadataBytes(), metadata)
    }

    func testExternalReplacementAfterWriteRemainsRetainedAndUnchanged() async throws {
        try await assertExternalChange(remove: false)
    }

    func testExternalRemovalAfterWriteIsNotResurrected() async throws {
        try await assertExternalChange(remove: true)
    }

    private func assertExternalChange(remove: Bool) async throws {
        let f = try ResourceRegistrationFixture()
        defer { f.backend.release() }
        let saved = try await f.beginApply(), metadata = try f.metadataBytes()
        var replacement = saved
        replacement.name = "External registration"; replacement.displayName = "External registration"
        if remove { try FileManager.default.removeItem(at: f.registration) }
        else { XCTAssertTrue(VMLibrary.save(replacement, rootURL: f.libraryRoot)) }
        let replacementBytes = remove ? nil : try Data(contentsOf: f.registration)
        f.library.reload()
        try await f.finishApply()
        XCTAssertEqual(f.model.config, saved)
        XCTAssertTrue(f.model.isRetained)
        let record = try XCTUnwrap(f.library.retainedControlRecords.first)
        XCTAssertTrue(record.descriptor.isActive)
        XCTAssertFalse(f.library.dismissRetainedControl(record.id))
        f.model.stop()
        XCTAssertEqual(f.backend.counts, [1, 1, 1], "Retained unowned controls must not regain legacy Stop")
        XCTAssertTrue(f.backend.isRunning())
        XCTAssertEqual(f.model.statusNote, "이 실행의 프로세스 소유권을 확인할 수 없어 정지할 수 없습니다.")
        if remove { XCTAssertFalse(FileManager.default.fileExists(atPath: f.registration.path)) }
        else { XCTAssertEqual(try Data(contentsOf: f.registration), replacementBytes) }
        XCTAssertEqual(try f.metadataBytes(), metadata)
    }
}
