import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeStartFeedbackTests: XCTestCase {
    private let configurationSentence = "VM 설정이 올바르지 않아 Windows를 시작하지 못했으니 VM 설정을 확인하거나 백업에서 복원하세요."
    private let appSentence = "앱 실행 파일이 없거나 손상되어 Windows를 시작하지 못했으니 BridgeVM을 다시 설치하세요."

    private var readinessSentences: [(String, String)] { [
        ("target-disk-missing", "Windows 디스크를 찾을 수 없어 시작하지 못했으니 VM 설정에서 디스크를 다시 선택하세요."),
        ("target-disk-access", "Windows 디스크에 접근할 수 없어 시작하지 못했으니 Finder에서 디스크 파일의 읽기 및 쓰기 권한을 확인하세요."),
        ("target-disk-empty", "Windows 디스크가 비어 있어 시작하지 못했으니 VM 설정에서 Windows가 설치된 디스크를 선택하세요."),
        ("uefi-vars-missing", "부팅 설정 파일을 찾을 수 없어 Windows를 시작하지 못했으니 VM 폴더에서 파일을 확인하거나 백업에서 복원하세요."),
        ("uefi-vars-access", "부팅 설정 파일에 접근할 수 없어 Windows를 시작하지 못했으니 Finder에서 해당 파일의 읽기 및 쓰기 권한을 확인하세요."),
        ("uefi-vars-size", "부팅 설정 파일의 크기가 올바르지 않아 Windows를 시작하지 못했으니 백업에서 64 MiB 크기의 파일을 복원하세요."),
        ("ram-range", "메모리 설정이 범위를 벗어나 Windows를 시작하지 못했으니 메모리를 1024~65536 MiB로 설정하세요."),
        ("cpu-range", "CPU 개수 설정이 범위를 벗어나 Windows를 시작하지 못했으니 CPU를 1~64개로 설정하세요."),
        ("share-invalid", "공유 폴더 설정이 올바르지 않아 Windows를 시작하지 못했으니 VM 설정에서 공유 폴더를 다시 선택하세요."),
        ("share-incomplete", "공유 폴더 설정이 완전하지 않아 Windows를 시작하지 못했으니 VM 설정에서 공유 폴더를 다시 선택하세요."),
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
            XCTAssertTrue(text.contains(selected == "ram-range" ? "1024~65536 MiB" : "1~64개"))
            for code in codes { XCTAssertTrue(diagnostics.contains { $0.contains("[\(code)]") }) }
        }
    }

    private struct LockedKeychain: VTPMStateKeyProviding {
        func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
            throw VTPMStateSecurityError.keychainRead(-25308)
        }
    }

    func testKeychainFailureIsActionableAndStatusStaysInDiagnosticLog() async throws {
        let f = try HvfGUIStartFixture(encrypted: true, typed: true)
        let config = f.base.config
        let root = f.root
        f.session.guiStartWorker = { input in
            await HvfGUIStartWorker.run(.init(config: config, repoRoot: root, policy: input.policy,
                keyProvider: LockedKeychain(), processIsRunning: { _ in false },
                launch: { _ in XCTFail("key refusal must prevent launch") },
                effectAdmission: input.effectAdmission, permit: input.permit))
        }
        let ticket = try f.start()
        try await f.wait { !ticket.workerPending }
        XCTAssertEqual(f.session.guiStartFailureText,
            "로그인 키체인에 접근할 수 없어 Windows를 시작하지 못했으니 키체인 잠금을 해제하고 다시 시도하세요.")
        XCTAssertFalse(f.session.guiStartFailureText?.contains("OSStatus") ?? true)
        XCTAssertTrue(f.session.events.contains { event in
            if case let .unknown(line) = event { return line.contains("keyAccess") && line.contains("-25308") }
            return false
        })
        XCTAssertEqual(f.effects.snapshot.launches, 0)
        await f.clean()
    }

    func testPreparationFailureIsActionableAndRawDetailStaysInDiagnosticLog() async throws {
        let f = try HvfGUIStartFixture(typed: true)
        try FileManager.default.createDirectory(atPath: f.base.config.ctlFilePath, withIntermediateDirectories: true)
        let ticket = try f.start()
        try await f.wait { !ticket.workerPending }
        XCTAssertEqual(f.session.guiStartFailureText,
            "VM 작업 파일을 준비하지 못했으니 VM 폴더의 접근 권한과 저장 장치의 여유 공간을 확인하고 다시 시도하세요.")
        XCTAssertFalse(f.session.guiStartFailureText?.contains("control path") ?? true)
        XCTAssertTrue(f.session.events.contains { event in
            if case let .unknown(line) = event { return line.contains("preparation") && line.contains("control path is not a regular file") }
            return false
        })
        XCTAssertEqual(f.effects.snapshot.launches, 0)
        await f.clean()
    }
}
