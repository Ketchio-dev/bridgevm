import Foundation

final class AppUIDriverAXFixture: AppUIDriverAXAccess {
    struct Node {
        var strings: [String: String] = [:]
        var related: [String: [Int]] = [:]
        var enabled = true
    }
    typealias A = AppUIDriverAXAttribute
    var nodes: [Int: Node] = [:]
    var presses: [Int] = []
    var sets = 0
    var keepSearch = false
    var failure: AppUIDriverAXError?
    var collision = false

    init() {
        nodes[0] = Node(related: [A.windows: [1]])
        add(1, role: "AXWindow", identifier: AppUIDriverConstants.mainWindowIdentifier)
        add(2, role: "AXSheet", identifier: AppUIDriverConstants.sheetWindowIdentifier)
        nodes[1]!.related = [A.children: [2, 3, 4, 5, 6, 7, 8, 9, 10]]
        nodes[2]!.related[A.children] = [11, 12, 13, 14, 15]
        add(3, role: "AXButton", identifier: "bridgevm.first-run.create")
        add(4, role: "AXButton", identifier: "bridgevm.first-run.import")
        add(5, role: "AXTextField", identifier: "bridgevm.first-run.name")
        add(6, role: "AXButton", identifier: "bridgevm.library.overview")
        add(7, role: "AXButton", title: "Amber, HVF, stopped")
        add(8, role: "AXButton", title: "Indigo, HVF, stopped")
        add(9, role: "AXTextField", identifier: "bridgevm.library.search", value: "")
        add(10, role: "AXButton", title: "검색 지우기")
        add(11, role: "AXButton", identifier: "bridgevm.create.commit")
        add(12, role: "AXStaticText", identifier: "bridgevm.create.windows.iso.selection", value: "")
        add(13, role: "AXStaticText", identifier: "bridgevm.create.windows.guest-payload.selection", value: "")
        add(14, role: "AXStaticText", identifier: "bridgevm.create.windows.guest-manifest.selection", value: "")
        add(15, role: "AXButton", title: "취소")
    }
    func add(_ id: Int, role: String, identifier: String? = nil, title: String? = nil, value: String? = nil) {
        var strings = [A.role: role]
        strings[A.identifier] = identifier; strings[A.title] = title; strings[A.value] = value
        nodes[id] = Node(strings: strings)
    }
    func application() -> Int { 0 }
    func related(_ node: Int, _ attribute: String) throws -> [Int] {
        if let failure { throw failure }
        return nodes[node]!.related[attribute, default: []]
    }
    func string(_ node: Int, _ attribute: String) throws -> String? { nodes[node]!.strings[attribute] }
    func enabled(_ node: Int) throws -> Bool { nodes[node]!.enabled }
    func press(_ node: Int) throws {
        presses.append(node)
        if node == 10 { nodes[9]!.strings[A.value] = "" }
    }
    func setSearch(_ node: Int) throws {
        sets += 1
        if !keepSearch { nodes[node]!.strings[A.value] = "Indigo" }
    }
    func hash(_ node: Int) -> UInt { collision ? 0 : UInt(node) }
    func same(_ lhs: Int, _ rhs: Int) -> Bool { lhs == rhs }
}
