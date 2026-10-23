import Foundation

extension AppUIDriverAXContracts {
    /// The seeded overview rows are found by their exact current LibraryWorkspaceList
    /// label (SwiftUI exposes it as AXDescription), exactly once each.
    static func overviewRowContracts() throws {
        typealias A = AppUIDriverAXAttribute
        let amber = "Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF", indigo = "Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF"
        check(AppUIDriverOverviewRows.amber == amber && AppUIDriverOverviewRows.indigo == indigo,
              "driver rows use the current row label format")
        let fixture = AppUIDriverAXFixture()
        fixture.add(50, role: "AXMenuButton", description: "Amber 작업")
        fixture.add(51, role: "AXMenuButton", description: "Indigo 작업")
        fixture.nodes[1]!.related[A.children]!.append(contentsOf: [50, 51])
        let rows = try AppUIDriverAXOperations(access: fixture).perform(.overviewCards).values
        check(rows?.amberVisible == true && rows?.indigoVisible == true, "both current rows observed once beside their menus")
        let titled = AppUIDriverAXFixture()
        titled.nodes[7]!.strings = [A.role: "AXButton", A.title: amber]
        try check(AppUIDriverAXOperations(access: titled).perform(.overviewCards).values?.amberVisible == true,
                  "an exact title is the same row")

        // A stale or different label never stands in for a seeded row.
        for label in ["Amber, HVF, stopped", "Amber, 실행 중, 4 vCPU  ·  4 GiB RAM  ·  HVF", "Amber, 정지됨", "Amber"] {
            let stale = AppUIDriverAXFixture()
            stale.nodes[7]!.strings[A.description] = label
            let values = try AppUIDriverAXOperations(access: stale).perform(.overviewCards).values
            check(values?.amberVisible == false && values?.indigoVisible == true, "label is not a seeded row: " + label)
        }
        let text = AppUIDriverAXFixture()
        text.nodes[7]!.strings[A.role] = "AXStaticText"
        try check(AppUIDriverAXOperations(access: text).perform(.overviewCards).values?.amberVisible == false,
                  "a label on a non-button is not a row")

        // Pro mode with no selection renders a second workspace list: every row is then ambiguous.
        let pro = AppUIDriverAXFixture()
        pro.add(60, role: "AXButton", description: amber)
        pro.add(61, role: "AXButton", description: indigo)
        pro.nodes[1]!.related[A.children]!.append(contentsOf: [60, 61])
        refuses(.ambiguousElement) { _ = try AppUIDriverAXOperations(access: pro).perform(.overviewCards) }
        refuses(.ambiguousElement) { _ = try AppUIDriverAXOperations(access: pro).perform(.searchState) }
        check(pro.presses.isEmpty && pro.sets == 0, "ambiguous rows are observations only")

        let filtered = AppUIDriverAXFixture()
        filtered.nodes[1]!.related[A.children]!.removeAll { $0 == 7 }
        filtered.nodes[9]!.strings[A.value] = "Indigo"
        let search = try AppUIDriverAXOperations(access: filtered).perform(.searchState).values
        check(search?.amberVisible == false && search?.indigoVisible == true && search?.searchValue == "Indigo",
              "filtered search observes only the Indigo row")
    }
}
