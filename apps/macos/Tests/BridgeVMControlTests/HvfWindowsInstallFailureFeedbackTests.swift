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
        XCTAssertEqual(text, "Could not import the Windows files. Check access to the disk image and firmware files and make sure the destination has enough free space.")
        XCTAssertFalse(text.contains("SeedError"))
        XCTAssertFalse(text.contains("error 4"))
        XCTAssertTrue(WindowsFileFailureFeedback.diagnostic("import", error).contains(String(reflecting: error)))
    }

    func testImportShowsFixedVHDXGuidanceWithoutPublishingDiagnosticDetails() {
        let guidance = [
            "This VHDX uses 4 KB logical sectors. Import Windows from a disk that uses 512-byte logical sectors.",
            "This differencing disk depends on a parent disk. Merge the chain in Hyper-V, then import the merged disk.",
            "This VHDX has pending changes. Attach it in Windows, detach it cleanly, then try again."
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
        let errors: [Error] = [cocoa, helper]
        for error in errors {
            XCTAssertTrue(WindowsFileFailureFeedback.isImportOutOfSpace(error))
            XCTAssertEqual(WindowsFileFailureFeedback.reportImport("import", error), helperMessage)
        }
        let unrelated = NSError(domain: "BridgeVM.MediaImport", code: 1,
                                userInfo: [NSLocalizedDescriptionKey: helperMessage + "\n/private/test/vars.fd"])
        XCTAssertEqual(WindowsFileFailureFeedback.reportImport("import", unrelated), WindowsFileFailureFeedback.importing)
        XCTAssertEqual(WindowsFileFailureFeedback.report("import-publication-sync", cocoa,
            sentence: WindowsFileFailureFeedback.unsynced), WindowsFileFailureFeedback.unsynced)
    }
}
