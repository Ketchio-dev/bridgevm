import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelResourceDisplayTests: XCTestCase {
    func testSuccessfulApplyPublishesSavedDisplayBeforeCompletion() async throws {
        try await checkSuccess(editDraft: false)
    }

    func testSuccessfulApplyPreservesNewDraftWhilePublishingSavedDisplay() async throws {
        try await checkSuccess(editDraft: true)
    }

    private func checkSuccess(editDraft: Bool) async throws {
        let fixture = try ResourceDisplayCompletionFixture()
        try await fixture.initializeDisplay()
        let completion = try await fixture.apply(editDraft: editDraft)
        try fixture.assertSaved(memoryMiB: 8192, cpu: 4)
        XCTAssertEqual(completion.config.memMiB, 8192)
        XCTAssertEqual(completion.config.cpuCount, 4)
        XCTAssertEqual(completion.memoryGiB, 8)
        XCTAssertEqual(completion.cpu, 4)
        XCTAssertEqual(completion.pendingMemoryGiB, editDraft ? 16 : 8)
        XCTAssertEqual(completion.pendingCPU, editDraft ? 8 : 4)
        XCTAssertEqual(fixture.model.statusNote, "리소스 적용됨: 8GB / 4코어")
    }

    func testFailedActualWriterPreservesDisplayFilesAndRequestedDraft() async throws {
        let fixture = try ResourceDisplayCompletionFixture(validHandoff: false)
        try await fixture.initializeDisplay()
        let completion = try await fixture.apply()
        XCTAssertEqual(completion.config, fixture.original)
        XCTAssertEqual(completion.memoryGiB, 4)
        XCTAssertEqual(completion.cpu, 2)
        XCTAssertEqual(completion.pendingMemoryGiB, 8)
        XCTAssertEqual(completion.pendingCPU, 4)
        XCTAssertEqual(try [fixture.registration, fixture.launch, fixture.handoff].map { try Data(contentsOf: $0) },
                       fixture.originalBytes)
        XCTAssertEqual(fixture.library.latestConfiguration(for: fixture.original), fixture.original)
        XCTAssertEqual(fixture.model.statusNote, "리소스 적용 실패 — VM은 재시작하지 않았습니다.")
    }

    func testUnchangedAllocationCompletesWithCoherentDisplay() async throws {
        let fixture = try ResourceDisplayCompletionFixture()
        try await fixture.initializeDisplay()
        let completion = try await fixture.apply(memoryGiB: 4, cpu: 2)
        try fixture.assertSaved(memoryMiB: 4096, cpu: 2)
        XCTAssertEqual(completion.memoryGiB, 4)
        XCTAssertEqual(completion.cpu, 2)
        XCTAssertEqual(completion.pendingMemoryGiB, 4)
        XCTAssertEqual(completion.pendingCPU, 2)
        XCTAssertEqual(fixture.model.statusNote, "리소스 적용됨: 4GB / 2코어")
    }

    func testSuccessfulApplyInvalidatesActualOldReadAdmittedDuringApply() async throws {
        let fixture = try ResourceDisplayCompletionFixture()
        try await fixture.initializeDisplay()
        let probe = ResourceDisplayStaleReadProbe()
        fixture.backend.install(probe)
        let apply = Task { try await fixture.apply(editDraft: true) }
        defer { probe.releaseAll(); fixture.backend.release() }
        guard await probe.waitForWriteEntry() else {
            probe.releaseAll(); _ = try? await apply.value
            return XCTFail("Writer did not reach its pre-write boundary")
        }
        // The current status-card Refresh action remains enabled during Apply.
        fixture.model.refresh()
        let readGeneration = fixture.model.refreshGeneration
        guard await probe.waitForReadCapture() else {
            probe.releaseAll(); _ = try? await apply.value
            return XCTFail("Actual reader did not capture the old allocation")
        }
        XCTAssertEqual(probe.snapshot, [4096, 2])
        probe.releaseWrite()
        let completion = try await apply.value
        try fixture.assertSaved(memoryMiB: 8192, cpu: 4)
        // The existing equality guard must reject the captured query after publication.
        XCTAssertNotEqual(completion.refreshGeneration, readGeneration)
        var memoryEmissions: [Double] = [], cpuEmissions: [Int] = []
        let memoryObserver = fixture.model.$memGiB.dropFirst().sink { memoryEmissions.append($0) }
        let cpuObserver = fixture.model.$cpu.dropFirst().sink { cpuEmissions.append($0) }
        defer { memoryObserver.cancel(); cpuObserver.cancel() }
        probe.releaseRead()
        let returned = await probe.waitForReadReturnBoundary()
        XCTAssertTrue(returned)
        // Supplemental bounded observation, not an independently awaited refresh task.
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertFalse(probe.gateTimedOut)
        XCTAssertTrue(memoryEmissions.allSatisfy { $0 == 8 }, "Old memory publication: \(memoryEmissions)")
        XCTAssertTrue(cpuEmissions.allSatisfy { $0 == 4 }, "Old CPU publication: \(cpuEmissions)")
        XCTAssertEqual(fixture.model.memGiB, 8)
        XCTAssertEqual(fixture.model.cpu, 4)
        XCTAssertEqual(fixture.model.pendingMemGiB, 16)
        XCTAssertEqual(fixture.model.pendingCPU, 8)
    }
}
