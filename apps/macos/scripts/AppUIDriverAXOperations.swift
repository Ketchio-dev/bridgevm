import Foundation

struct AppUIDriverAXOperations<Access: AppUIDriverAXAccess> {
    let access: Access
    /// Restarts the read-only pass only; the mutation and its readback below run once.
    var restart = AppUIDriverAXRestart.never()
    typealias A = AppUIDriverAXAttribute
    private enum Step { case observed(AppUIDriverValues), press(Access.Element), search(Access.Element) }

    func perform(_ operation: AppUIDriverOperation) throws -> AppUIDriverAXResult {
        switch try restart.run({ try locate(operation) }) {
        case .observed(let values): return AppUIDriverAXResult(outcome: .observed, values: values)
        case .press(let node):
            try access.press(node)
            return AppUIDriverAXResult(outcome: .performed, values: nil)
        case .search(let node):
            try access.setSearch(node)
            guard try access.string(node, A.value) == "Indigo" else { throw AppUIDriverFailure.textReadbackMismatch }
            return AppUIDriverAXResult(outcome: .performed, values: AppUIDriverValues(searchValue: "Indigo"))
        }
    }

    private func locate(_ operation: AppUIDriverOperation) throws -> Step {
        guard let root = try window(sheet: operation == .createForm || operation == .cancelCreate) else {
            if operation == .createForm { return .observed(AppUIDriverValues(commitVisible: false)) }
            throw AppUIDriverFailure.windowUnavailable
        }
        let nodes = try AppUIDriverAXGraph.walk(root, children: { node in
            try access.related(node, A.children).filter { child in
                let role = try access.string(child, A.role)
                return role != "AXWindow" && role != "AXSheet"
            }
        }, hash: access.hash, same: access.same)
        switch operation {
        case .welcomeControls:
            return .observed(AppUIDriverValues(
                createVisible: try identified("bridgevm.first-run.create", nodes) != nil,
                importVisible: try identified("bridgevm.first-run.import", nodes) != nil))
        case .pressCreate: return try button(identified("bridgevm.first-run.create", nodes))
        case .createForm: return .observed(try createForm(nodes))
        case .cancelCreate: return try button(labelled("취소", nodes))
        case .pressImport: return try button(identified("bridgevm.first-run.import", nodes))
        case .importForm:
            return .observed(AppUIDriverValues(nameVisible: try identified("bridgevm.first-run.name", nodes) != nil))
        case .pressOverview: return try button(identified("bridgevm.library.overview", nodes))
        case .overviewCards:
            return .observed(AppUIDriverValues(amberVisible: try labelled(AppUIDriverOverviewRows.amber, nodes) != nil,
                                               indigoVisible: try labelled(AppUIDriverOverviewRows.indigo, nodes) != nil))
        case .setSearchIndigo:
            let node = try required("bridgevm.library.search", nodes)
            guard try access.string(node, A.role) == "AXTextField" else { throw AppUIDriverFailure.invalidElement }
            guard try access.enabled(node) else { throw AppUIDriverFailure.elementDisabled }
            return .search(node)
        case .clearSearch: return try button(labelled("검색 지우기", nodes))
        case .searchState:
            let node = try required("bridgevm.library.search", nodes)
            guard let value = try access.string(node, A.value) else { throw AppUIDriverFailure.invalidElement }
            return .observed(AppUIDriverValues(amberVisible: try labelled(AppUIDriverOverviewRows.amber, nodes) != nil,
                indigoVisible: try labelled(AppUIDriverOverviewRows.indigo, nodes) != nil, searchValue: value))
        }
    }

    private func window(sheet: Bool) throws -> Access.Element? {
        let windows = try access.related(access.application(), A.windows)
        guard let main = try AppUIDriverAXGraph.unique(windows, matching: {
            try access.string($0, A.identifier) == AppUIDriverConstants.mainWindowIdentifier
                && access.string($0, A.role) == "AXWindow"
        }) else { throw AppUIDriverFailure.windowUnavailable }
        if !sheet { return main }
        // Sheets are descendants through the public AXChildren relationship.
        // Do not enter another window/sheet to find a similarly named descendant.
        let descendants = try AppUIDriverAXGraph.walk(main, children: { node in
            if !access.same(node, main) {
                let role = try access.string(node, A.role)
                if role == "AXWindow" || role == "AXSheet" { return [] }
            }
            return try access.related(node, A.children)
        }, hash: access.hash, same: access.same)
        return try AppUIDriverAXGraph.unique(descendants, matching: {
            guard try access.string($0, A.identifier) == AppUIDriverConstants.sheetWindowIdentifier else { return false }
            let role = try access.string($0, A.role)
            return role == "AXSheet" || role == "AXWindow"
        })
    }

    private func identified(_ identifier: String, _ nodes: [Access.Element]) throws -> Access.Element? {
        try AppUIDriverAXGraph.unique(nodes) { try access.string($0, A.identifier) == identifier }
    }
    private func required(_ identifier: String, _ nodes: [Access.Element]) throws -> Access.Element {
        guard let result = try identified(identifier, nodes) else { throw AppUIDriverFailure.elementNotFound }
        return result
    }
    /// A button by its exact title or description, such as a seeded overview row by
    /// its current label; a second match (two lists) is ambiguous.
    private func labelled(_ label: String, _ nodes: [Access.Element]) throws -> Access.Element? {
        try AppUIDriverAXGraph.unique(nodes, matching: { node in
            guard try access.string(node, A.role) == "AXButton" else { return false }
            return try access.string(node, A.title) == label || access.string(node, A.description) == label
        })
    }
    /// The checks before a press are reads of the pass; the press itself is not.
    private func button(_ node: Access.Element?) throws -> Step {
        guard let node else { throw AppUIDriverFailure.elementNotFound }
        guard try access.string(node, A.role) == "AXButton" else { throw AppUIDriverFailure.invalidElement }
        guard try access.enabled(node) else { throw AppUIDriverFailure.elementDisabled }
        return .press(node)
    }
    private func createForm(_ nodes: [Access.Element]) throws -> AppUIDriverValues {
        guard try identified("bridgevm.create.commit", nodes) != nil else { return AppUIDriverValues(commitVisible: false) }
        func selection(_ field: String) throws -> String {
            let node = try required("bridgevm.create.windows.\(field).selection", nodes)
            guard let value = try access.string(node, A.value) else { throw AppUIDriverFailure.invalidElement }
            return value
        }
        return AppUIDriverValues(commitVisible: true, isoSelection: try selection("iso"),
                                 payloadSelection: try selection("guest-payload"),
                                 manifestSelection: try selection("guest-manifest"))
    }
}
