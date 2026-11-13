import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallFailureFeedbackTests: XCTestCase {
    func testPreparationErrorsNeverExposeTypesCodesOrInternalDescriptions() async throws {
        let errors: [Error] = [
            HvfWindowsBootSeed.SeedError.gptUnreadable, HvfWindowsBootSeed.SeedError.espNotFound,
            HvfWindowsBootSeed.SeedError.varstoreUnreadable, HvfWindowsBootSeed.SeedError.varstoreFull,
            HvfWindowsBootSeed.SeedError.secureBootManifestMissing,
            HvfWindowsBootSeed.SeedError.secureBootManifestInvalid("raw detail"),
            HvfWindowsBootSeed.SeedError.secureBootConflict("raw detail"),
            HvfWindowsInstallFinalizationError.invalidState("raw detail"),
            HvfWindowsInstallFinalizationError.unsafePath("/private/test"),
            HvfWindowsInstallFinalizationError.unsupportedJournal,
            HvfWindowsInstallFinalizationError.missingArtifact("/private/test"),
            HvfWindowsInstallFinalizationError.transactionBusy,
            VTPMStateSecurityError.keychainRead(-25308),
            VTPMImportedRecoveryError.destinationIdentityExists("test"),
            HvfWindowsInstallSourceLock.LockError.unavailable,
            CocoaError(.fileWriteNoPermission), NSError(domain: NSPOSIXErrorDomain, code: 13),
            FirstRunImport.ValidationError.emptyName
        ]
        for error in errors {
            let plan = HvfWindowsInstallTestSupport.plan()
            defer { try? FileManager.default.removeItem(at: plan.repoRoot) }
            try FileManager.default.createDirectory(at: URL(fileURLWithPath: plan.sourceImagePath).deletingLastPathComponent(), withIntermediateDirectories: true)
            let process = HvfWindowsInstallCancellationProcess(gate: HvfWindowsInstallCancellationGate(), holdProcess: false)
            let session = HvfWindowsInstallSession(plan: plan, execution: .init(process: process,
                verifySource: { _ in true }, prepareMedia: { _ in throw error }))
            session.attemptActive = true
            await session.run()
            XCTAssertEqual(session.stage, .failed("Windows 설치 파일을 준비하지 못했으니 BridgeVM을 다시 설치하고 VM 폴더의 접근 권한과 여유 공간을 확인한 뒤 다시 시도하세요."), String(reflecting: error))
            XCTAssertTrue(session.logLines.contains { $0.contains(String(reflecting: error)) }, "raw diagnostic retained")
        }
    }

    func testImportFailurePresentationKeepsRawDetailOnlyInDiagnostics() {
        let error = HvfWindowsBootSeed.SeedError.secureBootManifestMissing
        let text = WindowsFileFailureFeedback.report("import", error, sentence: WindowsFileFailureFeedback.importing)
        XCTAssertEqual(text, "Windows 파일을 가져오지 못했으니 디스크와 부팅 설정 파일의 접근 권한과 저장 장치의 여유 공간을 확인하고 다시 시도하세요.")
        XCTAssertFalse(text.contains("SeedError"))
        XCTAssertFalse(text.contains("error 4"))
        XCTAssertTrue(WindowsFileFailureFeedback.diagnostic("import", error).contains(String(reflecting: error)))
    }
}
