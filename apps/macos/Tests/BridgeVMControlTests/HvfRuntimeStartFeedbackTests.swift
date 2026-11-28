import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeStartFeedbackTests: XCTestCase {
    private let configurationSentence = "BridgeVM couldn’t start Windows with these settings, so check the VM settings or restore them from a backup."
    private let appSentence = "Reinstall BridgeVM to restore a missing or damaged app component."

    private var readinessSentences: [(String, String)] { [
        ("target-disk-missing", "To recover the missing Windows disk, reconnect its drive or restore the disk file to its original location."),
        ("target-disk-access", "BridgeVM can’t access the Windows disk, so check its read and write permissions in Finder."),
        ("target-disk-empty", "The Windows disk file is empty, so restore the original disk from a backup to the same location."),
        ("uefi-vars-missing", "Look for the missing Windows boot settings file in the VM folder or restore it from a backup."),
        ("uefi-vars-access", "BridgeVM can’t access the Windows boot settings, so check the file’s read and write permissions in Finder."),
        ("uefi-vars-size", "The Windows boot settings file has the wrong size, so restore the 64 MiB file from a backup."),
        ("ram-range", "Set the VM’s memory between 1024 and 65536 MiB, then start Windows again."),
        ("cpu-range", "Choose 1 to 64 CPUs in VM settings, then start Windows again."),
        ("share-invalid", "Choose the shared folder again in VM settings, then start Windows again."),
        ("share-incomplete", "To complete the shared folder setup, choose the folder again in VM settings."),
        ("boot-wrapper-missing", appSentence), ("signed-runner-missing", appSentence),
        ("swtpm-missing", appSentence), ("swtpm-signature", appSentence),
        ("watchdog-range", configurationSentence), ("evidence-not-directory", configurationSentence),
        ("control-path-collision", configurationSentence), ("vtpm-path-collision", configurationSentence),
        ("vtpm-key-id", configurationSentence), ("vtpm-state-not-directory", configurationSentence)
    ] }

    func testEveryReadinessCodeHasSpecificSafeFeedbackAndDiagnosticCode() throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        let session = f.session()
        // Keep this table in step with the launch boundary, including internal blockers.
        let source = try String(contentsOf: URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Sources/BridgeVMControl/HvfEngine/HvfEngineConfig.swift"), encoding: .utf8)
        let expression = try NSRegularExpression(pattern: "launch\\(\\\"([^\\\"]+)\\\"")
        let codes = expression.matches(in: source, range: NSRange(source.startIndex..., in: source))
            .compactMap { Range($0.range(at: 1), in: source).map { String(source[$0]) } }
        XCTAssertEqual(Set(codes), Set(readinessSentences.map { $0.0 }))
        for (code, expected) in readinessSentences {
            var diagnostics: [String] = []
            // An arbitrary summary proves wording cannot choose the category.
            let failure = HvfRuntimeLaunchReadiness.failure(blockers: [
                .init(code: code, scope: .launch, summary: "changed summary")
            ], diagnostic: { diagnostics.append($0) })
            let ticket = HvfGUIStartOperation(configuration: session.config)
            session.guiStartOperation = ticket
            ticket.finish(failure)
            let text = try XCTUnwrap(session.guiStartFailureText)
            XCTAssertEqual(text, expected, code)
            for forbidden in ["vTPM", "UEFI vars", "RAW", "wrapper", "probe", "swtpm", "entitlement", "RAM"] {
                XCTAssertFalse(text.contains(forbidden), "\(code): \(text)")
            }
            XCTAssertTrue(diagnostics.contains { $0.contains("[\(code)]") }, code)
        }
    }

    /// A keychain status or an internal blocker summary stays in the run's event log; only an
    /// unexpected launch error is shown, after the sentence.
    func testOtherFailuresShowOneSentenceWithoutTheRawError() {
        let launch = HvfRuntimeStartOutcome.failed(.processLaunch, "The file “hvf-runner” couldn’t be opened.")
        XCTAssertEqual(launch.failureText, "BridgeVM couldn’t start the VM process, so quit and reopen BridgeVM and try again.\nThe file “hvf-runner” couldn’t be opened.")
        let cases: [(HvfRuntimeStartFailureStage, String, String)] = [
            (.keyAccess, "The operation couldn’t be completed. (OSStatus -25291.)",
             "BridgeVM couldn’t use this VM’s security key in your login keychain, so check that the keychain is unlocked and start Windows again."),
            (.readiness(["relocation-pending"]), "The move has not finished recovering. Check both bundles.",
             "BridgeVM couldn’t start Windows yet, so choose Refresh and try again."),
            (.readiness(["install-cleanup-pending"]), "Windows installation cleanup is incomplete.",
             "BridgeVM couldn’t start Windows yet, so choose Refresh and try again."),
        ]
        for (stage, detail, expected) in cases {
            let text = HvfRuntimeStartOutcome.failed(stage, detail).failureText
            XCTAssertEqual(text, expected, "\(stage)")
            XCTAssertFalse(text?.contains(detail) ?? true, "\(stage)")
        }
    }

    func testReadinessShowsFirstActionableBlockerAndRetainsAllDiagnostics() throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        let session = f.session()
        for codes in [["swtpm-missing", "ram-range", "cpu-range"], ["cpu-range", "ram-range"]] {
            var diagnostics: [String] = []
            let failure = HvfRuntimeLaunchReadiness.failure(blockers: codes.map {
                .init(code: $0, scope: .launch, summary: "arbitrary wording")
            }, diagnostic: { diagnostics.append($0) })
            let ticket = HvfGUIStartOperation(configuration: session.config)
            session.guiStartOperation = ticket
            ticket.finish(failure)
            let text = try XCTUnwrap(session.guiStartFailureText)
            let selected = codes.first == "cpu-range" ? "cpu-range" : "ram-range"
            XCTAssertEqual(text, readinessSentences.first { $0.0 == selected }?.1)
            XCTAssertTrue(text.contains(selected == "ram-range" ? "1024 and 65536 MiB" : "1 to 64 CPUs"))
            for code in codes { XCTAssertTrue(diagnostics.contains { $0.contains("[\(code)]") }) }
        }
    }

    private struct RefusingKeychain: VTPMStateKeyProviding {
        let status: OSStatus
        func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
            throw VTPMStateSecurityError.keychainRead(status)
        }
    }

    /// A locked keychain (-25308) asks for an unlock; Deny (-25293) or Cancel (-128) on the macOS
    /// password prompt after an app update means the keychain is unlocked but access was refused.
    func testKeychainFailureIsActionableAndStatusStaysInDiagnosticLog() async throws {
        let unlock = "Unlock your login keychain and try starting Windows again."
        let allow = "BridgeVM needs permission to use this VM’s security key. Start Windows again, enter your Mac password and choose Always Allow."
        for (status, sentence) in [(OSStatus(-25308), unlock), (-25293, allow), (-128, allow)] {
            let f = try HvfGUIStartFixture(encrypted: true, typed: true)
            let config = f.base.config
            let root = f.root
            f.session.guiStartWorker = { input in
                await HvfGUIStartWorker.run(.init(config: config, repoRoot: root, policy: input.policy,
                    keyProvider: RefusingKeychain(status: status), processIsRunning: { _ in false },
                    launch: { _ in XCTFail("key refusal must prevent launch") },
                    effectAdmission: input.effectAdmission, permit: input.permit))
            }
            let ticket = try f.start()
            try await f.wait { !ticket.workerPending }
            XCTAssertEqual(f.session.guiStartFailureText, sentence, "OSStatus \(status)")
            XCTAssertFalse(f.session.guiStartFailureText?.contains("OSStatus") ?? true)
            XCTAssertTrue(f.session.events.contains { event in
                if case let .unknown(line) = event { return line.contains("keyAccess") && line.contains("\(status)") }
                return false
            })
            XCTAssertEqual(f.effects.snapshot.launches, 0)
            await f.clean()
        }
    }

    func testPreparationFailureIsActionableAndRawDetailStaysInDiagnosticLog() async throws {
        let f = try HvfGUIStartFixture(typed: true)
        try FileManager.default.createDirectory(atPath: f.base.config.ctlFilePath, withIntermediateDirectories: true)
        let ticket = try f.start()
        try await f.wait { !ticket.workerPending }
        XCTAssertEqual(f.session.guiStartFailureText,
            "BridgeVM couldn’t prepare the VM files, so check access to the VM folder and the available disk space.")
        XCTAssertFalse(f.session.guiStartFailureText?.contains("control path") ?? true)
        XCTAssertTrue(f.session.events.contains { event in
            if case let .unknown(line) = event { return line.contains("preparation") && line.contains("control path is not a regular file") }
            return false
        })
        XCTAssertEqual(f.effects.snapshot.launches, 0)
        await f.clean()
    }
}
