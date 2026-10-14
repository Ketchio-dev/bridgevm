import Foundation

enum AppUIInstallCancelAXFixtures {
    typealias A = AppUIDriverAXAttribute
    static let install = 40, start = 41, cancel = 42, stage = 43, failure = 44

    static func make(stage: String = "대기", title: String = "설치 시작", startEnabled: Bool = true,
                     cancelVisible: Bool = false, cancelEnabled: Bool = false, failure: String? = nil) -> AppUIDriverAXFixture {
        let fixture = AppUIDriverAXFixture()
        fixture.add(install, role: "AXGroup", identifier: "bridgevm.windows.install.view")
        fixture.add(start, role: "AXButton", identifier: "bridgevm.install.start", title: title)
        fixture.add(cancel, role: "AXButton", identifier: "bridgevm.windows.install.cancel", title: "취소")
        fixture.add(Self.stage, role: "AXStaticText", identifier: "bridgevm.windows.install.stage", value: stage)
        fixture.nodes[start]!.enabled = startEnabled
        fixture.nodes[cancel]!.enabled = cancelEnabled
        fixture.nodes[1]!.related[A.children] = [install, start, Self.stage] + (cancelVisible ? [cancel] : [])
        if let failure {
            fixture.add(Self.failure, role: "AXStaticText", identifier: "bridgevm.windows.install.failure", value: failure)
            fixture.nodes[1]!.related[A.children]!.append(Self.failure)
        }
        return fixture
    }
}
