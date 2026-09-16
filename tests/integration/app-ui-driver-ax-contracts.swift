import Foundation

@main
enum AppUIDriverAXContracts {
    static var checks = 0
    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) rethrows {
        checks += 1
        if try !condition() { fatalError("AX contract: " + name) }
    }
    static func refuses(_ expected: AppUIDriverFailure, _ body: () throws -> Void) {
        checks += 1
        do { try body(); fatalError("expected refusal " + expected.rawValue) }
        catch let error as AppUIDriverFailure { if error != expected { fatalError("wrong refusal: \(error)") } }
        catch { fatalError("untyped refusal: \(error)") }
    }
    static func main() throws {
        typealias A = AppUIDriverAXAttribute
        let fixture = AppUIDriverAXFixture()
        let operations = AppUIDriverAXOperations(access: fixture)
        var seen: Set<AppUIDriverOperation> = []
        for operation in AppUIDriverOperation.allCases {
            let result = try operations.perform(operation)
            seen.insert(operation)
            check(result.outcome == (AppUIDriverOperationPolicy.isMutation(operation) ? .performed : .observed),
                  "fixed operation outcome \(operation)")
        }
        check(seen.count == 11 && fixture.presses == [3, 15, 4, 6, 10] && fixture.sets == 1,
              "eleven fixed operations and six one-shot mutations")
        let welcome = try operations.perform(.welcomeControls).values
        check(welcome?.createVisible == true && welcome?.importVisible == true, "welcome controls observed")
        let form = try operations.perform(.createForm).values
        check(form?.commitVisible == true && form?.isoSelection == "" && form?.payloadSelection == ""
              && form?.manifestSelection == "", "selection values come from observed nodes")
        try check(operations.perform(.importForm).values?.nameVisible == true, "import name observed")
        let cards = try operations.perform(.overviewCards).values
        check(cards?.amberVisible == true && cards?.indigoVisible == true, "two exact cards observed")
        try check(operations.perform(.searchState).values?.searchValue == "", "clear readback observed")

        let missing = AppUIDriverAXFixture()
        missing.nodes[1]!.related[A.children]!.removeAll { $0 == 2 }
        let pendingSheet = try AppUIDriverAXOperations(access: missing).perform(.createForm).values
        check(pendingSheet?.presentFields == ["commitVisible"] && pendingSheet?.commitVisible == false,
              "not yet attached sheet observes only missing commit")
        refuses(.windowUnavailable) { _ = try AppUIDriverAXOperations(access: missing).perform(.cancelCreate) }
        missing.nodes[1]!.related[A.children]!.append(2)
        missing.nodes[2]!.related[A.children] = []
        let absent = try AppUIDriverAXOperations(access: missing).perform(.createForm).values
        check(absent?.commitVisible == false && absent?.presentFields == ["commitVisible"],
              "missing commit does not manufacture source selections")
        missing.nodes[2]!.related[A.children] = [11]
        refuses(.elementNotFound) { _ = try AppUIDriverAXOperations(access: missing).perform(.createForm) }

        let ambiguous = AppUIDriverAXFixture()
        ambiguous.add(20, role: "AXButton", identifier: "bridgevm.first-run.create")
        ambiguous.nodes[1]!.related[A.children]!.append(20)
        refuses(.ambiguousElement) { _ = try AppUIDriverAXOperations(access: ambiguous).perform(.pressCreate) }
        check(ambiguous.presses.isEmpty, "ambiguous controls never mutate")
        let disabled = AppUIDriverAXFixture()
        disabled.nodes[3]!.enabled = false
        refuses(.elementDisabled) { _ = try AppUIDriverAXOperations(access: disabled).perform(.pressCreate) }
        check(disabled.presses.isEmpty, "disabled controls never mutate")
        disabled.nodes[3]!.strings[A.role] = "AXStaticText"
        refuses(.invalidElement) { _ = try AppUIDriverAXOperations(access: disabled).perform(.pressCreate) }

        let wrongText = AppUIDriverAXFixture()
        wrongText.keepSearch = true
        refuses(.textReadbackMismatch) { _ = try AppUIDriverAXOperations(access: wrongText).perform(.setSearchIndigo) }
        check(wrongText.sets == 1, "failed text readback never retries mutation")
        let wrongWindow = AppUIDriverAXFixture()
        wrongWindow.nodes[1]!.strings[A.identifier] = "unrelated-window"
        refuses(.windowUnavailable) { _ = try AppUIDriverAXOperations(access: wrongWindow).perform(.pressCreate) }
        wrongWindow.nodes[1]!.strings[A.identifier] = AppUIDriverConstants.mainWindowIdentifier
        wrongWindow.add(21, role: "AXWindow", identifier: AppUIDriverConstants.mainWindowIdentifier)
        wrongWindow.nodes[0]!.related[A.windows]!.append(21)
        refuses(.ambiguousElement) { _ = try AppUIDriverAXOperations(access: wrongWindow).perform(.pressCreate) }

        let isolated = AppUIDriverAXFixture()
        isolated.add(22, role: "AXWindow", identifier: "other")
        isolated.add(23, role: "AXButton", identifier: "bridgevm.first-run.create")
        isolated.nodes[22]!.related[A.children] = [23]
        isolated.nodes[1]!.related[A.children]!.append(22)
        _ = try AppUIDriverAXOperations(access: isolated).perform(.pressCreate)
        check(isolated.presses == [3], "foreign descendant window is not searched")
        isolated.nodes[2]!.strings[A.identifier] = "unrelated-sheet"
        refuses(.windowUnavailable) { _ = try AppUIDriverAXOperations(access: isolated).perform(.cancelCreate) }

        let nested = AppUIDriverAXFixture()
        nested.add(30, role: "AXGroup")
        nested.nodes[30]!.related[A.children] = [2]
        nested.nodes[1]!.related[A.children]!.removeAll { $0 == 2 }
        nested.nodes[1]!.related[A.children]!.append(30)
        nested.add(31, role: "AXWindow", identifier: "foreign")
        nested.add(32, role: "AXSheet", identifier: AppUIDriverConstants.sheetWindowIdentifier)
        nested.nodes[31]!.related[A.children] = [32]
        nested.nodes[1]!.related[A.children]!.append(31)
        try check(AppUIDriverAXOperations(access: nested).perform(.createForm).values?.commitVisible == true,
                  "nested attached sheet selected via children; foreign window is not entered")
        nested.nodes[1]!.related[A.children]!.append(32)
        refuses(.ambiguousElement) { _ = try AppUIDriverAXOperations(access: nested).perform(.cancelCreate) }

        let aliased = AppUIDriverAXFixture()
        aliased.nodes[1]!.related[A.children]!.append(contentsOf: [1, 3, 3])
        aliased.collision = true
        _ = try AppUIDriverAXOperations(access: aliased).perform(.pressCreate)
        check(aliased.presses == [3], "identity deduplicates aliases and cycles despite hash collisions")
        let walked = try AppUIDriverAXGraph.walk(0, limit: 4, children: { $0 < 3 ? [$0 + 1] : [] },
                                                hash: { UInt($0) }, same: ==)
        check(walked == [0, 1, 2, 3], "bounded complete graph")
        refuses(.protocolLimit) {
            _ = try AppUIDriverAXGraph.walk(0, limit: 3, children: { [$0 + 1] }, hash: { UInt($0) }, same: ==)
        }
        let fault = AppUIDriverAXFixture()
        fault.failure = AppUIDriverAXError(failure: .axFailure, rawValue: -25204)
        do { _ = try AppUIDriverAXOperations(access: fault).perform(.pressCreate); fatalError("missing AX failure") }
        catch let error as AppUIDriverAXError { check(error.rawValue == -25204, "raw AX error preserved") }
        check(fault.presses.isEmpty, "transport error has no fallback mutation")
        print("PASS: \(checks) app UI driver AX checks; injected graph only, no GUI or permission query")
    }
}
