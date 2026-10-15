import Foundation

enum AppUIInstallCancelAXRefusalContracts {
    static func run() throws {
        typealias A = AppUIDriverAXAttribute
        typealias F = AppUIInstallCancelAXFixtures
        typealias T = AppUIInstallCancelAXContracts
        for operation in [AppUIInstallCancelOperation.start, .cancel, .retry] {
            let stage = operation == .start ? "대기" : operation == .cancel ? "설치 입력 확인" : "취소됨"
            let target = operation == .cancel ? F.cancel : F.start
            let identifier = operation == .cancel ? "bridgevm.windows.install.cancel" : "bridgevm.install.start"
            let ambiguous = F.make(stage: stage, cancelVisible: true, cancelEnabled: true)
            ambiguous.add(50, role: "AXButton", identifier: identifier)
            ambiguous.nodes[1]!.related[A.children]!.append(50)
            T.refuses(.ambiguousElement, fixture: ambiguous, operation: operation)
            let wrongRole = F.make(stage: stage, cancelVisible: true, cancelEnabled: true)
            wrongRole.nodes[target]!.strings[A.role] = "AXStaticText"
            T.refuses(.invalidElement, fixture: wrongRole, operation: operation)
            let disabled = F.make(stage: stage, cancelVisible: true, cancelEnabled: true)
            disabled.nodes[target]!.enabled = false
            T.refuses(.elementDisabled, fixture: disabled, operation: operation)
            let stale = F.make(stage: "실패", cancelVisible: true, cancelEnabled: true)
            T.refuses(.outOfOrder, fixture: stale, operation: operation)
            let failure = F.make(stage: stage, cancelVisible: true, cancelEnabled: true, failure: "합성 UI 검증 종료")
            T.refuses(.outOfOrder, fixture: failure, operation: operation)
            let missing = F.make(stage: stage, cancelVisible: true, cancelEnabled: true)
            missing.nodes[1]!.related[A.children]!.removeAll { $0 == target }
            T.refuses(.elementNotFound, fixture: missing, operation: operation)
        }
        let absentInstall = F.make()
        absentInstall.nodes[1]!.related[A.children]!.removeAll { $0 == F.install }
        T.refuses(.elementNotFound, fixture: absentInstall, operation: .start)
        let duplicateStage = F.make()
        duplicateStage.add(50, role: "AXStaticText", identifier: "bridgevm.windows.install.stage", value: "대기")
        duplicateStage.nodes[1]!.related[A.children]!.append(50)
        T.refuses(.ambiguousElement, fixture: duplicateStage, operation: .start)
        let wrongWindow = F.make()
        wrongWindow.nodes[1]!.strings[A.role] = "AXGroup"
        T.refuses(.windowUnavailable, fixture: wrongWindow, operation: .start)
        let duplicateWindow = F.make()
        duplicateWindow.add(50, role: "AXWindow", identifier: AppUIDriverConstants.mainWindowIdentifier)
        duplicateWindow.nodes[0]!.related[A.windows]!.append(50)
        T.refuses(.ambiguousElement, fixture: duplicateWindow, operation: .start)
        let isolated = F.make()
        for (node, role) in [(50, "AXWindow"), (51, "AXSheet")] {
            isolated.add(node, role: role)
            isolated.add(node + 10, role: "AXButton", identifier: "bridgevm.install.start")
            isolated.nodes[node]!.related[A.children] = [node + 10]
            isolated.nodes[1]!.related[A.children]!.append(node)
        }
        _ = try AppUIInstallCancelAXOperations(access: isolated).perform(.start)
        T.check(isolated.presses == [F.start], "foreign windows and sheets cannot supply install buttons")
        let aliased = F.make()
        aliased.nodes[1]!.related[A.children]!.append(contentsOf: [1, F.start, F.start])
        aliased.collision = true
        _ = try AppUIInstallCancelAXOperations(access: aliased).perform(.start)
        T.check(aliased.presses == [F.start], "aliases and cycles do not fabricate duplicate controls")
        let fault = F.make()
        fault.failure = AppUIDriverAXError(failure: .axFailure, rawValue: -25204)
        do {
            _ = try AppUIInstallCancelAXOperations(access: fault).perform(.start)
            fatalError("missing AX transport failure")
        } catch let error as AppUIDriverAXError {
            T.check(error.failure == .axFailure && error.rawValue == -25204, "AX transport failure preserves raw value")
        }
        T.check(fault.presses.isEmpty && fault.sets == 0, "AX transport failure never retries mutation")
    }
}
