import Foundation

@main
enum AppUIInstallCancelAXContracts {
    typealias A = AppUIDriverAXAttribute
    typealias F = AppUIInstallCancelAXFixtures
    static var checks = 0
    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) rethrows {
        checks += 1
        if try !condition() { fatalError("install AX contract: " + name) }
    }
    static func refuses(_ expected: AppUIDriverFailure, fixture: AppUIDriverAXFixture,
                        operation: AppUIInstallCancelOperation) {
        checks += 1
        do {
            _ = try AppUIInstallCancelAXOperations(access: fixture).perform(operation)
            fatalError("expected refusal " + expected.rawValue)
        } catch let error as AppUIDriverFailure {
            if error != expected { fatalError("wrong refusal: \(error)") }
        } catch { fatalError("untyped refusal: \(error)") }
        check(fixture.presses.isEmpty && fixture.sets == 0, "refusal precedes every mutation")
    }
    static func main() throws {
        var presses: [Int] = []
        let actions: [(AppUIInstallCancelOperation, String, String, Int)] = [
            (.start, "대기", "설치 시작", F.start),
            (.cancel, "설치 입력 확인", "설치 중…", F.cancel),
            (.retry, "취소됨", "다시 시도", F.start)]
        for (operation, stage, title, button) in actions {
            let fixture = F.make(stage: stage, title: title, cancelVisible: operation == .cancel, cancelEnabled: true)
            let result = try AppUIInstallCancelAXOperations(access: fixture).perform(operation)
            check(result.outcome == .performed && result.values == nil, "each allowed press has mutation-only result")
            check(fixture.presses == [button] && fixture.sets == 0, "only exact install button is pressed once")
            presses += fixture.presses
        }
        check(presses == [F.start, F.cancel, F.start], "three fixed operations press start cancel retry")
        try reads()
        try AppUIInstallCancelAXRefusalContracts.run()
        print("PASS: \(checks) install cancellation AX checks; injected graph only, no GUI or permission query")
    }

    private static func reads() throws {
        let cases: [(String, String, Bool, Bool, Bool, String?)] = [
            ("대기", "설치 시작", true, false, false, nil),
            ("설치 입력 확인", "설치 중…", false, true, true, nil),
            ("취소 중…", "설치 중…", false, true, false, nil),
            ("취소됨", "다시 시도", true, false, false, nil),
            ("실패", "다시 시도", true, false, false, "합성 UI 검증 종료")]
        for (stage, title, startEnabled, cancelVisible, cancelEnabled, failure) in cases {
            let fixture = F.make(stage: stage, title: title, startEnabled: startEnabled,
                cancelVisible: cancelVisible, cancelEnabled: cancelEnabled, failure: failure)
            let result = try AppUIInstallCancelAXOperations(access: fixture).perform(.readState)
            let expected = AppUIInstallCancelValues(installVisible: true, startVisible: true, startEnabled: startEnabled,
                cancelVisible: cancelVisible, cancelEnabled: cancelEnabled, stage: stage, title: title, failure: failure)
            check(result.outcome == .observed && result.values == expected, "raw enabled title stage and failure are observed")
            check(fixture.presses.isEmpty && fixture.sets == 0, "state query performs no mutation")
        }
        let missing = F.make()
        missing.nodes[1]!.related[A.children] = []
        let result = try AppUIInstallCancelAXOperations(access: missing).perform(.readState)
        check(result.outcome == .observed && result.values == .init(installVisible: false, startVisible: false,
            startEnabled: false, cancelVisible: false, cancelEnabled: false, stage: "", title: ""),
            "missing controls yield observed false instead of invented labels")
        check(missing.presses.isEmpty, "missing read never presses")
        let raw = F.make(stage: " 대기\n", title: " 설치 시작\n")
        let observed = try AppUIInstallCancelAXOperations(access: raw).perform(.readState).values
        check(observed?.stage == " 대기\n" && observed?.title == " 설치 시작\n", "raw strings are not trimmed or normalized")
        let fallback = F.make()
        fallback.nodes[F.start]!.strings[A.title] = nil
        fallback.nodes[F.start]!.strings[A.description] = "설치 시작"
        try check(AppUIInstallCancelAXOperations(access: fallback).perform(.readState).values?.title == "설치 시작",
            "button description fallback remains observed text")
    }
}
