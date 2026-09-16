import Testing
@testable import BridgeVMProductE2E

struct T17FileChooserPredicateDiagnosticsTests {
    private struct Node {
        let identity: Int
        let id: String?
        let role: String?
    }
    private enum Failure: Error, Equatable { case privateText(String) }
    private let secret = "/private/operator/\u{D55C}/private-name"
    private func lookup(_ observer: T17FileChooserPredicateDiagnostics, _ nodes: [Node],
                        id: String = "GoToWindow", roles: Set<String> = ["AXSheet"]) throws -> Node? {
        try observer.find(in: { nodes }, id: id, roles: roles,
                          metadata: { ($0.id, $0.role) }, same: { $0.identity == $1.identity })
    }
    @Test func absentIdentifierAndRejectedRoleRemainDifferent() throws {
        let observer = T17FileChooserPredicateDiagnostics()
        observer.beginOwner(panelCached: true)
        #expect(try lookup(observer, [Node(identity: 1, id: secret, role: "AXSheet")]) == nil)
        #expect(observer.owner.idMatches == 0)
        #expect(observer.owner.roleMatches == 0)
        observer.beginOwner(panelCached: true)
        #expect(throws: T17FileChooser.failure("identified chooser element has unexpected role")) {
            _ = try lookup(observer, [Node(identity: 2, id: "GoToWindow", role: "AXWindow")])
        }
        #expect(observer.owner.idMatches == 1)
        #expect(observer.owner.roleMatches == 0)
        #expect(observer.owner.state == .matchError)
        #expect(observer.owner.nodes == 1 && observer.owner.visited == 1)
        #expect(observer.owner.scope == .application)
        #expect(!observer.field.attempted && observer.field.visited == nil)
        #expect(observer.field.scope == .notReached)
    }

    @Test func bothExistingPathFieldRolesAreAcceptedInOwnerScope() throws {
        for role in ["AXTextField", "AXComboBox"] {
            let observer = T17FileChooserPredicateDiagnostics()
            observer.beginOwner(panelCached: true)
            #expect(try lookup(observer, [Node(identity: 1, id: "GoToWindow", role: "AXSheet")])?.identity == 1)
            let node = try lookup(observer, [Node(identity: 2, id: "PathTextField", role: role)],
                                  id: "PathTextField", roles: ["AXTextField", "AXComboBox"])
            #expect(node?.identity == 2)
            #expect(observer.field.scope == .goToSheet && observer.field.attempted)
            #expect(observer.field.idMatches == 1 && observer.field.roleMatches == 1)
            #expect(observer.owner.roleMatches == 1 && observer.field.state == .complete)
        }
    }

    @Test func duplicateIdentityAndAmbiguityKeepOriginalReadsAndResults() {
        for secondIdentity in [1, 2] {
            let nodes = [Node(identity: 1, id: "GoToWindow", role: "AXSheet"),
                         Node(identity: secondIdentity, id: "GoToWindow", role: "AXSheet")]
            var baselineReads: [Int] = [], observedReads: [Int] = []
            var baselineSame: [String] = [], observedSame: [String] = []
            var baseline: Int?, observed: Int?
            var baselineFailed = false, observedFailed = false, walks = 0
            do {
                baseline = try T17FileChooserIdentity.find(in: nodes, id: "GoToWindow", roles: ["AXSheet"],
                    metadata: { baselineReads.append($0.identity); return ($0.id, $0.role) },
                    same: { baselineSame.append("\($0.identity)/\($1.identity)"); return $0.identity == $1.identity })?.identity
            } catch { baselineFailed = true }
            let observer = T17FileChooserPredicateDiagnostics()
            observer.beginOwner(panelCached: true)
            do {
                observed = try observer.find(in: { walks += 1; return nodes }, id: "GoToWindow", roles: ["AXSheet"],
                    metadata: { observedReads.append($0.identity); return ($0.id, $0.role) },
                    same: { observedSame.append("\($0.identity)/\($1.identity)"); return $0.identity == $1.identity })?.identity
            } catch { observedFailed = true }
            #expect(walks == 1 && baselineReads == observedReads && baselineSame == observedSame)
            #expect(baseline == observed && baselineFailed == observedFailed)
            #expect(observedFailed == (secondIdentity == 2))
            #expect(observer.owner.state == (observedFailed ? .matchError : .complete))
        }
    }

    @Test func interruptedWalkAndTreeFailureNeverReportAZeroResult() {
        let observer = T17FileChooserPredicateDiagnostics()
        observer.beginOwner(panelCached: true)
        var reads = 0
        do {
            let _: Node? = try observer.find(in: {
                #expect(observer.owner.state == .walking && observer.owner.attempted)
                #expect(observer.owner.nodes == nil && observer.owner.visited == nil)
                throw Failure.privateText(secret)
            }, id: "GoToWindow", roles: ["AXSheet"], metadata: { (node: Node) in
                reads += 1
                return (node.id, node.role)
            }, same: { $0.identity == $1.identity })
            Issue.record("Expected the original walk failure")
        } catch { #expect(error as? Failure == .privateText(secret)) }
        #expect(reads == 0 && observer.owner.state == .treeError)
        #expect(observer.owner.nodes == nil && observer.owner.idMatches == nil)
        #expect(observer.owner.errors == nil && !observer.field.attempted)
        #expect(!observer.snapshot.contains(secret))
    }

    @Test func metadataFailureRetainsPartialCountsWithoutPrivateErrorText() {
        let observer = T17FileChooserPredicateDiagnostics()
        observer.beginOwner(panelCached: true)
        let nodes = [Node(identity: 1, id: secret, role: "AXSheet"),
                     Node(identity: 2, id: "GoToWindow", role: "AXSheet")]
        do {
            let _: Node? = try observer.find(in: { nodes }, id: "GoToWindow", roles: ["AXSheet"],
                metadata: { node in
                    if node.identity == 2 { throw Failure.privateText(secret) }
                    return (node.id, node.role)
                }, same: { $0.identity == $1.identity })
            Issue.record("Expected the original metadata failure")
        } catch { #expect(error as? Failure == .privateText(secret)) }
        #expect(observer.owner.state == .metadataError && observer.owner.errors == 1)
        #expect(observer.owner.nodes == 2 && observer.owner.visited == 1)
        #expect(observer.owner.idMatches == 0 && observer.owner.roleMatches == 0)
        #expect(!observer.field.attempted && observer.field.idMatches == nil)
        #expect(!observer.snapshot.contains(secret))
    }

    @Test func nextOwnerPollClearsPriorFieldEvenWhenPanelIsMissing() throws {
        let observer = T17FileChooserPredicateDiagnostics()
        observer.beginOwner(panelCached: true)
        _ = try lookup(observer, [Node(identity: 1, id: "GoToWindow", role: "AXSheet")])
        _ = try lookup(observer, [Node(identity: 2, id: "PathTextField", role: "AXTextField")],
                       id: "PathTextField", roles: ["AXTextField", "AXComboBox"])
        #expect(observer.field.attempted)
        observer.beginOwner(panelCached: false)
        #expect(observer.panelCached == false)
        #expect(!observer.owner.attempted && observer.owner.state == .notReached)
        #expect(!observer.field.attempted && observer.field.state == .notReached)
        #expect(observer.owner.nodes == nil && observer.field.idMatches == nil)
        observer.beginOwner(panelCached: true)
        _ = try lookup(observer, [])
        #expect(observer.owner.idMatches == 0 && !observer.field.attempted)
    }

    @Test func unknownLookupKeepsOriginalBehaviorWithoutRecordingPrivateIdentifiers() throws {
        let observer = T17FileChooserPredicateDiagnostics()
        let node = try lookup(observer, [Node(identity: 1, id: secret, role: "AXSheet")], id: secret)
        #expect(node?.identity == 1)
        #expect(!observer.owner.attempted && !observer.field.attempted)
        #expect(!observer.snapshot.contains(secret))
    }

    @Test func fixedAsciiContextKeepsPredicatesAndKeyWithinGlobalBound() throws {
        let observer = T17FileChooserPredicateDiagnostics()
        observer.beginOwner(panelCached: true)
        _ = try lookup(observer, [Node(identity: 1, id: secret, role: secret)])
        let key = "phase=activation;pair_posted=false"
        let context = observer.context(activation: false, key: key,
                                       timeout: String(repeating: "redacted", count: 200), tree: "AXSheet/none")
        #expect(context.count == 900)
        #expect(context.hasPrefix("predicate{panel_cached=true;owner{scope=application"))
        #expect(context.contains(key) && context.contains("id_matches=0"))
        #expect(context.utf8.allSatisfy { $0 < 128 })
        #expect(!context.contains(secret))
    }
}
