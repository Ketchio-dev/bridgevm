import XCTest
@testable import BridgeVMProductE2E

final class A9ImportEndStateTests: XCTestCase {
    private let keyExists = "VM 번들을 만들지 못했습니다: 대상 VM ID에 이미 vTPM 키가 있습니다: windows-11"

    func testStageLabelsMapToFixedTokens() {
        XCTAssertEqual(A9ImportEndState.stageToken("VM 파일 복사 중…"), "copying")
        XCTAssertEqual(A9ImportEndState.stageToken("라이브러리 확인 중…"), "reading-library")
        XCTAssertEqual(A9ImportEndState.stageToken("새 단계"), "other")
        XCTAssertEqual(A9ImportEndState.stageToken(nil), "none")
    }

    func testErrorsMapToFixedTokensWithoutProductText() {
        XCTAssertEqual(A9ImportEndState.errorToken(keyExists), "bundle-failed/destination-key-exists")
        XCTAssertEqual(A9ImportEndState.errorToken("VM 번들을 만들지 못했습니다: Keychain에서 vTPM 상태 키를 읽지 못했습니다 (OSStatus -25293)."),
                       "bundle-failed/keychain-read/status-25293")
        XCTAssertEqual(A9ImportEndState.errorToken("VM 번들을 만들지 못했습니다: The file “vm.json” couldn’t be saved."), "bundle-failed/other")
        XCTAssertEqual(A9ImportEndState.errorToken("디스크 이미지를 찾을 수 없습니다: /Volumes/private/disk.raw"), "validation")
        XCTAssertEqual(A9ImportEndState.errorToken("VM 파일과 등록 정보는 보존했지만 저장 완료를 확인하지 못했습니다."), "published-unsynced")
        let unknown = A9ImportEndState.errorToken("예상하지 못한 오류 /Users/someone/private")
        XCTAssertEqual(unknown, "unclassified/bytes\("예상하지 못한 오류 /Users/someone/private".utf8.count)")
        XCTAssertEqual(A9ImportEndState.errorToken(nil), "none")
    }

    func testDescriptionCountsLaneRegistrations() {
        let texts = [A9ImportEndState.error: keyExists, A9ImportEndState.commit: "가져오기"]
        XCTAssertEqual(A9ImportEndState.describe(texts, registrations: ["windows-11"], expected: "bridgevm-a9-import-lane-1-aaaaaaaaaaaa"),
            "import_state=v1,stage=none,error=bundle-failed/destination-key-exists,commit=true,recover=false,lane_registrations=1,expected_registered=false")
        XCTAssertEqual(A9ImportEndState.describe(nil, registrations: nil, expected: "x"),
                       "import_state=v1,screen=unreadable,lane_registrations=unreadable")
    }

    func testMissingDashboardCarriesTheImportEndState() throws {
        let fixture = try A9CleanupFixture(); defer { try? FileManager.default.removeItem(at: fixture.allocation.parent) }
        let library = URL(fileURLWithPath: fixture.request.libraryRootPath).appendingPathComponent("windows-11")
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: library.appendingPathComponent("vm.json"))
        let ui = FailedImportScreenUI(texts: [A9ImportEndState.error: keyExists, A9ImportEndState.commit: "가져오기"])
        XCTAssertThrowsError(try A9ImportRuntimeNavigation.openRuntimeControls(ui) {
            A9ImportEndState.capture(ui, request: fixture.request, fileManager: .default)
        }) {
            let blocker = $0 as? T17Blocker
            XCTAssertEqual(blocker?.code, "ui-element-missing")
            XCTAssertEqual(blocker?.detail, "bridgevm.dashboard.advanced; import_state=v1,stage=none,"
                + "error=bundle-failed/destination-key-exists,commit=true,recover=false,lane_registrations=1,expected_registered=false")
        }
        XCTAssertEqual(ui.presses, ["bridgevm.dashboard.advanced@1200.0"])
    }

    func testOtherFailuresPassUnchanged() {
        let ui = FailedImportScreenUI(texts: [:]); ui.failure = T17Blocker(code: "accessibility-untrusted", detail: "untrusted")
        XCTAssertThrowsError(try A9ImportRuntimeNavigation.openRuntimeControls(ui) { "import_state=v1" }) {
            XCTAssertEqual($0 as? T17Blocker, T17Blocker(code: "accessibility-untrusted", detail: "untrusted"))
        }
    }
}

/// The first-run screen after a failed import: no dashboard, no runtime view.
private final class FailedImportScreenUI: T17UIControlling {
    let texts: [String: String]; var failure: T17Blocker?; var presses: [String] = []
    init(texts: [String: String]) { self.texts = texts }
    private func missing(_ id: String) -> T17Blocker { T17Blocker(code: "ui-element-missing", detail: id) }
    func press(_ identifier: String, timeout: TimeInterval) throws {
        presses.append("\(identifier)@\(timeout)"); throw failure ?? missing(identifier)
    }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { texts.filter { identifiers.contains($0.key) } }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func expand(_ identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { throw missing(identifier) }
    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval) throws { throw missing("display") }
    func textSnapshot() -> [String] { [] }
}
