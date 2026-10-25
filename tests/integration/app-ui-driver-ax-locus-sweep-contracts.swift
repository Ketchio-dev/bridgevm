import Foundation

/// Where each stage of the pass places its element; what never gets a locus; that
/// the recorder changes nothing else; that the failure path reads only roles and
/// names; and the bounds.
extension AppUIDriverAXLocusContracts {
    static func lookupWalkAndSheetLoci() {
        func locus(_ fake: Fake, _ operation: AppUIDriverOperation, _ node: Int, _ attribute: String) -> L? {
            fake.faults = [.init(node: node, attribute: attribute, status: -25201)]
            let outcome = run(fake, operation)
            guard outcome.restarts == 0, admitted(outcome.error, operation),
                  refusal(outcome.error)?.context?.distinctLoci == 1 else { return nil }
            return refusal(outcome.error)?.context?.locus
        }
        let app = nested()
        app.names[0] = ["AXRole", "AXTitle", "AXWindows"]
        check(locus(app, .pressOverview, 0, A.windows) == L(depth: -1, indexPath: [], visited: 0, role: "AXApplication",
              subrole: L.absent, parentRole: L.absent, attributeNames: ["AXRole", "AXTitle", "AXWindows"], identifierListed: false),
              "the application's window list: depth -1, no index and no parent")
        check(locus(nested(), .pressOverview, 1, A.identifier) == L(depth: -1, indexPath: [0], visited: 0, role: "AXWindow",
              subrole: "AXStandardWindow", parentRole: "AXApplication", attributeNames: [], identifierListed: false),
              "a window lookup entry: depth -1 at its AXWindows index")
        check(locus(nested(), .pressOverview, 1, A.children) == L(depth: 0, indexPath: [], visited: 0, role: "AXWindow",
              subrole: "AXStandardWindow", parentRole: "AXApplication", attributeNames: [], identifierListed: false),
              "the main window's own children: depth 0 before any visit")
        // The walk reads the roles of 1's children in order; 5 is index 3.
        check(locus(nested(), .pressOverview, 5, A.role) == L(depth: 1, indexPath: [3], visited: 1, role: L.unreadable,
              subrole: L.absent, parentRole: "AXWindow", attributeNames: [], identifierListed: false),
              "a child's role in the walk: one step down, after one visit")
        check(locus(Fake(), .pressOverview, 6, "AXEnabled") == L(depth: 1, indexPath: [4], visited: 9, role: "AXButton",
              subrole: L.absent, parentRole: "AXWindow", attributeNames: [], identifierListed: false),
              "the enabled check before the press is a pass read")
        // The sheet walk visits 1 and 3...10 and the main walk 2 and 11...15; 13 is index 2 of the sheet, index 0 of 1.
        check(locus(Fake(), .createForm, 13, A.identifier) == L(depth: 2, indexPath: [0, 2], visited: 15, role: "AXStaticText",
              subrole: L.absent, parentRole: "AXSheet", attributeNames: [], identifierListed: false),
              "a sheet's element is placed from the main window through the sheet")
        check(locus(Fake(), .overviewCards, 7, A.description) == L(depth: 1, indexPath: [5], visited: 9, role: "AXButton",
              subrole: L.absent, parentRole: "AXWindow", attributeNames: [], identifierListed: false),
              "an observation's label read is a pass read; the locus never reads the label")
    }

    static func failuresOutsideThePassHaveNoLocus() {
        let press = Fake()
        press.pressStatus = -25200
        check(refusal(run(press, .pressOverview).error)?.context == context(.performAction, "none", nil, nil) && press.presses == 1,
              "a press refusal has no locus")
        let late = Fake()
        late.pressStatus = -25202
        late.faults = [.init(node: 8, attribute: A.identifier, status: -25200, passes: 1...1)]
        let after = run(late, .pressOverview)
        check(refusal(after.error)?.context == context(.performAction, "none", nil, nil) && after.restarts == 1,
              "a press refused after a located restart has no locus")
        let search = Fake()
        search.searchStatus = -25204
        check(refusal(run(search, .setSearchIndigo).error)?.context == context(.setAttributeValue, A.value, nil, nil),
              "a search value refusal has no locus")
        let readback = Fake()
        readback.faults = [.init(node: 9, attribute: A.value, status: -25204)]
        let read = run(readback, .setSearchIndigo)
        check(refusal(read.error)?.context == context(.copyAttributeValue, A.value, nil, nil) && readback.searches == 1,
              "the readback after the search value has no locus")
        check(admitted(read.error, .setSearchIndigo), "a refusal without a locus stays a valid reply")
        let missing = Fake()
        missing.graph.nodes[1]!.related[A.children]!.removeAll { $0 == 6 }
        check(run(missing, .pressOverview).error as? AppUIDriverFailure == .elementNotFound, "a non-AX refusal is unchanged")
        let expired = Fake()
        expired.deadline = expired.time
        check(run(expired, .pressOverview).error as? AppUIDriverFailure == .deadlineExceeded, "a deadline refusal is unchanged")
    }

    /// The same request without the recorder: equal results, restarts and reads, apart from the locus reads.
    static func theRecorderChangesNothingElse() {
        for operation in AppUIDriverOperation.allCases {
            let located = Fake(), plain = Fake()
            let a = run(located, operation), b = run(plain, operation, located: false)
            check(a.result?.outcome == b.result?.outcome && a.result?.values == b.result?.values && a.error == nil
                  && located.reads == plain.reads && located.presses == plain.presses && located.searches == plain.searches,
                  "\(operation) is unchanged by the recorder")
        }
        for faults in [[Fake.Fault(node: 8, attribute: A.identifier, status: -25200, passes: 1...1)],
                       [Fake.Fault(node: 22, attribute: A.identifier, status: -25200)]] {
            let located = nested(), plain = nested()
            located.deadline = located.time + 0.11; plain.deadline = located.deadline
            located.faults = faults; plain.faults = faults
            let a = run(located, .pressOverview), b = run(plain, .pressOverview, located: false)
            var stripped = refusal(a.error)?.context
            stripped?.locus = nil; stripped?.distinctLoci = nil
            check(a.restarts == b.restarts && located.pass == plain.pass && a.result?.outcome == b.result?.outcome
                  && refusal(a.error)?.rawValue == refusal(b.error)?.rawValue && stripped == refusal(b.error)?.context
                  && located.reads.filter { !$0.afterFailure } == plain.reads && located.presses == plain.presses,
                  "restarts, passes, presses and the status are unchanged (\(faults[0].node!))")
        }
    }

    /// Every pass read of every operation, failing: after the failure only AXRole and AXSubrole of the element,
    /// AXRole of its parent and the element's names are read, and nothing is mutated.
    static func failurePathReadsOnlyRolesAndNames() {
        var parent: [Int: Int] = [1: 0]
        for (node, value) in Fake().graph.nodes { for child in value.related[A.children, default: []] { parent[child] = node } }
        var sites = 0
        for operation in AppUIDriverOperation.allCases {
            let probe = Fake()
            _ = run(probe, operation, located: false)
            var seen: [String] = []
            for read in probe.reads where !seen.contains("\(read.node) \(read.attribute)") {
                seen.append("\(read.node) \(read.attribute)")
                let fake = Fake()
                fake.faults = [.init(node: read.node, attribute: read.attribute, status: -25201)]
                let outcome = run(fake, operation)
                let after = fake.reads.filter { $0.afterFailure }
                if refusal(outcome.error)?.context?.locus == nil {
                    check(operation == .setSearchIndigo && read.node == 9 && read.attribute == A.value,
                          "only the readback after the search value has no locus (\(operation) \(read.node) \(read.attribute))")
                    check(after.isEmpty, "no locus read without a locus")
                    continue
                }
                sites += 1
                let allowed = [(read.node, A.role), (read.node, "AXSubrole"), (read.node, Fake.names)] + (parent[read.node].map { [($0, A.role)] } ?? [])
                check(after.allSatisfy { r in allowed.contains { $0.0 == r.node && $0.1 == r.attribute } }
                      && after.filter { $0.attribute == Fake.names }.count == 1,
                      "\(operation) \(read.node) \(read.attribute): the failure path reads only roles and names")
                check(fake.presses == 0 && fake.searches == 0 && admitted(outcome.error, operation),
                      "\(operation) \(read.node) \(read.attribute): no mutation and a valid reply")
            }
        }
        check(sites >= 100, "the sweep covers every pass read of every operation (\(sites) sites)")
    }

    static func bounds() {
        let deep = Fake()
        var parentNode = 1, next = 100, path: [Int] = []
        for level in 0..<40 {
            var children: [Int] = []
            for _ in 0..<(level % 3) { deep.graph.add(next, role: "AXStaticText"); children.append(next); next += 1 }
            deep.graph.add(next, role: "AXGroup")
            children.append(next)
            path.append(parentNode == 1 ? 9 + level % 3 : level % 3)
            deep.graph.nodes[parentNode]!.related[A.children, default: []].append(contentsOf: children)
            parentNode = next; next += 1
        }
        deep.faults = [.init(node: parentNode, attribute: A.identifier, status: -25201)]
        let located = refusal(run(deep, .pressOverview).error)
        check(located?.context?.locus == L(depth: 40, indexPath: Array(path.prefix(32)), visited: 88, role: "AXGroup",
              subrole: L.absent, parentRole: "AXGroup", attributeNames: [], identifierListed: false),
              "a path 40 steps deep keeps depth 40 and its first 32 indices")
        check(admitted(located, .pressOverview), "the truncated path is a valid reply")
        let many = nested()
        let extra = (0..<70).map { "AXA" + String(repeating: "a", count: $0 / 26) + String(UnicodeScalar(UInt8(97 + $0 % 26))) }.sorted()
        many.names[22] = extra.reversed() + ["AXIdentifier", "AXRole", "title", "AXRole"]
        many.faults = [.init(node: 22, attribute: A.identifier, status: -25201)]
        let capped = refusal(run(many, .pressOverview).error)
        check(capped?.context?.locus?.attributeNames == Array(extra.prefix(64)) && capped?.context?.locus?.identifierListed == true,
              "more than 64 names keep the first 64 sorted; AXIdentifier past them is still listed")
        check(admitted(capped, .pressOverview), "the capped names are a valid reply")
        let wide = Fake()
        wide.graph.nodes[1]!.related[A.children] = Array(1_000..<1_300)
        for id in 1_000..<1_300 { wide.graph.add(id, role: "AXCell") }
        wide.faults = [.init(node: nil, attribute: "AXProbe", status: -25200)]
        let access = AppUIDriverAXLocusAccess(wide, names: wide.attributeNames)
        var counts: [Int] = []
        for id in 1_000..<1_300 {
            let windows = (try? access.related(access.application(), A.windows)) ?? []
            _ = try? access.related(windows[0], A.children)
            do { _ = try access.string(id, "AXProbe") } catch { counts.append(refusal(error)?.context?.distinctLoci ?? 0) }
        }
        check(counts == Array(1...L.maximumDistinct) + Array(repeating: L.maximumDistinct, count: 300 - L.maximumDistinct),
              "the distinct count saturates at \(L.maximumDistinct)")
    }
}
