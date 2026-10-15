import Foundation

struct AppUIInstallCancelAXResult {
    let outcome: AppUIDriverOutcome
    let values: AppUIInstallCancelValues?
}

struct AppUIInstallCancelAXOperations<Access: AppUIDriverAXAccess> {
    let access: Access
    typealias A = AppUIDriverAXAttribute

    func perform(_ operation: AppUIInstallCancelOperation) throws -> AppUIInstallCancelAXResult {
        let windows = try access.related(access.application(), A.windows)
        guard let window = try AppUIDriverAXGraph.unique(windows, matching: {
            try access.string($0, A.identifier) == AppUIDriverConstants.mainWindowIdentifier
                && access.string($0, A.role) == "AXWindow"
        }) else { throw AppUIDriverFailure.windowUnavailable }
        let nodes = try AppUIDriverAXGraph.walk(window, children: { node in
            try access.related(node, A.children).filter {
                let role = try access.string($0, A.role)
                return role != "AXWindow" && role != "AXSheet"
            }
        }, hash: access.hash, same: access.same)
        let install = try identified("bridgevm.windows.install.view", nodes)
        let start = try identified("bridgevm.install.start", nodes)
        let cancel = try identified("bridgevm.windows.install.cancel", nodes)
        let stage = try text("bridgevm.windows.install.stage", nodes) ?? ""
        let title = try start.map { try text($0) } ?? ""
        let failure = try text("bridgevm.windows.install.failure", nodes)
        if operation == .readState {
            return .init(outcome: .observed, values: .init(installVisible: install != nil,
                startVisible: start != nil, startEnabled: try start.map { try access.enabled($0) } ?? false,
                cancelVisible: cancel != nil, cancelEnabled: try cancel.map { try access.enabled($0) } ?? false,
                stage: stage, title: title, failure: failure))
        }
        guard install != nil else { throw AppUIDriverFailure.elementNotFound }
        let expectedStage = operation == .start ? "대기" : operation == .cancel ? "설치 입력 확인" : "취소됨"
        guard stage == expectedStage, failure == nil else { throw AppUIDriverFailure.outOfOrder }
        guard let button = operation == .cancel ? cancel : start else { throw AppUIDriverFailure.elementNotFound }
        guard try access.string(button, A.role) == "AXButton" else { throw AppUIDriverFailure.invalidElement }
        guard try access.enabled(button) else { throw AppUIDriverFailure.elementDisabled }
        try access.press(button)
        return .init(outcome: .performed, values: nil)
    }

    private func identified(_ name: String, _ nodes: [Access.Element]) throws -> Access.Element? {
        try AppUIDriverAXGraph.unique(nodes) { try access.string($0, A.identifier) == name }
    }
    private func text(_ name: String, _ nodes: [Access.Element]) throws -> String? {
        guard let node = try identified(name, nodes) else { return nil }
        return try text(node)
    }
    private func text(_ node: Access.Element) throws -> String {
        for attribute in [A.value, A.title, A.description] {
            if let value = try access.string(node, attribute), !value.isEmpty { return value }
        }
        return ""
    }
}
