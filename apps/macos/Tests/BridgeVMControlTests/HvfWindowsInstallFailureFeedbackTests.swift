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
            XCTAssertEqual(session.stage, .failed("BridgeVM couldn’t prepare the Windows installation files. Check that the VM folder is writable and the drive has free space. If the problem persists, reinstall BridgeVM and retry."), String(reflecting: error))
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

    func testImportShowsFixedVHDXGuidanceWithoutPublishingDiagnosticDetails() {
        let guidance = [
            "이 VHDX는 4K 논리 섹터 디스크라 BridgeVM에서 부팅할 수 없으니 512바이트 논리 섹터 디스크에 설치한 Windows를 가져오세요.",
            "이 디스크는 다른 디스크를 기반으로 하는 차이 디스크이니 Hyper-V에서 부모 디스크와 병합한 뒤 다시 가져오세요.",
            "이 VHDX에는 반영되지 않은 변경 기록이 남아 있으니 Windows에서 이 VHDX를 한 번 연결했다가 분리한 뒤 다시 시도하세요."
        ]
        for detail in guidance {
            let error = NSError(domain: "BridgeVM.MediaImport", code: 1, userInfo: [NSLocalizedDescriptionKey: detail])
            XCTAssertEqual(WindowsFileFailureFeedback.report("import", error, sentence: WindowsFileFailureFeedback.importing), detail)
        }
        let path = "/private/test/disk.vhdx"
        let error = NSError(domain: "BridgeVM.MediaImport", code: 1,
                            userInfo: [NSLocalizedDescriptionKey: guidance[0] + "\n" + path])
        XCTAssertEqual(WindowsFileFailureFeedback.report("import", error, sentence: WindowsFileFailureFeedback.importing),
                       WindowsFileFailureFeedback.importing)
        XCTAssertTrue(WindowsFileFailureFeedback.diagnostic("import", error).contains(path))
    }

    func testImportSpaceRecoveryUsesTypedCopyAndKnownHelperErrors() {
        let cocoa = CocoaError(.fileWriteOutOfSpace)
        let helperMessage = "No space left on device. Free space on the destination volume and try again."
        let helper = NSError(domain: "BridgeVM.MediaImport", code: 1, userInfo: [NSLocalizedDescriptionKey: helperMessage])
        let guidance = "대상 볼륨의 공간이 부족해 디스크를 가져오지 못했습니다. 공간을 확보한 뒤 다시 시도하세요."
        let errors: [Error] = [cocoa, helper]
        for error in errors {
            XCTAssertTrue(WindowsFileFailureFeedback.isImportOutOfSpace(error))
            XCTAssertEqual(WindowsFileFailureFeedback.reportImport("import", error), guidance)
        }
        let unrelated = NSError(domain: "BridgeVM.MediaImport", code: 1,
                                userInfo: [NSLocalizedDescriptionKey: helperMessage + "\n/private/test/vars.fd"])
        XCTAssertEqual(WindowsFileFailureFeedback.reportImport("import", unrelated), WindowsFileFailureFeedback.importing)
        XCTAssertEqual(WindowsFileFailureFeedback.report("import-publication-sync", cocoa,
            sentence: WindowsFileFailureFeedback.unsynced), WindowsFileFailureFeedback.unsynced)
    }
}
