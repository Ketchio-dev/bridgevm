import Foundation
@testable import BridgeVMProductE2E

/// The product's settings layout: clipboard, network, shared folder and snapshots are in
/// the 설정 sheet, opened from the VM dashboard; Start is in the runtime view behind 고급. While
/// the sheet is open only its controls answer (Start is refused), and after 완료 it stays on
/// screen for one more poll while it animates away.
final class T17SettingsSheetProductUI: T17UIControlling {
    enum Screen { case dashboard, runtime }
    var screen = Screen.runtime
    var events: [String] = []
    var chooseError: Error?
    var guestShare = "C:\\bridgevm-share"
    private(set) var sheetOpen = false
    private var closingPolls: Int?
    private var restoreAsked = false
    private var snapshotStatus = "VM을 종료한 뒤 디스크와 UEFI vars를 한 쌍으로 저장할 수 있습니다."

    private static let sheetControls: Set<String> = [
        "bridgevm.runtime.settings", "bridgevm.runtime.settings.done",
        "bridgevm.runtime.settings.sharing", "bridgevm.runtime.settings.network", "bridgevm.runtime.settings.snapshots",
        "bridgevm.runtime.clipboard", "bridgevm.runtime.network", "bridgevm.runtime.share.enabled",
        "bridgevm.runtime.share.host.choose", "bridgevm.runtime.share.guest",
        "bridgevm.runtime.snapshot.create", "bridgevm.runtime.snapshot.restore", "bridgevm.runtime.snapshot.status",
    ]

    private func shown(_ id: String) -> Bool {
        if sheetOpen { return Self.sheetControls.contains(id) || (id == "bridgevm.runtime.snapshot.confirm" && restoreAsked) }
        switch screen {
        case .dashboard: return ["bridgevm.runtime.settings.open", "bridgevm.dashboard.advanced"].contains(id)
        case .runtime: return ["bridgevm.windows.runtime.view", "bridgevm.windows.runtime.start", "bridgevm.dashboard.back"].contains(id)
        }
    }

    private func require(_ id: String) throws {
        guard shown(id) else {
            throw T17Blocker(code: "ui-element-missing", detail: "required accessibility identifier was not found: \(id)")
        }
    }

    func press(_ identifier: String, timeout: TimeInterval) throws {
        try require(identifier); events.append("press \(identifier)")
        switch identifier {
        case "bridgevm.dashboard.back": screen = .dashboard
        case "bridgevm.dashboard.advanced": screen = .runtime
        case "bridgevm.runtime.settings.open": sheetOpen = true
        case "bridgevm.runtime.settings.done": closingPolls = 1
        case "bridgevm.runtime.snapshot.create": snapshotStatus = "스냅샷 생성 완료"
        case "bridgevm.runtime.snapshot.restore": restoreAsked = true
        case "bridgevm.runtime.snapshot.confirm": restoreAsked = false; snapshotStatus = "스냅샷 복원 완료"
        default: break
        }
    }

    func waitFor(_ identifier: String, timeout: TimeInterval) throws {
        try require(identifier); events.append("wait \(identifier)")
    }

    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {
        try require(identifier); events.append("toggle \(identifier) \(enabled)")
    }

    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {
        try require(identifier); events.append("choose \(path) \(identifier)")
        if let chooseError { throw chooseError }
    }

    func text(_ identifier: String, timeout: TimeInterval) throws -> String {
        try require(identifier); events.append("text \(identifier)")
        return identifier == "bridgevm.runtime.share.guest" ? guestShare : snapshotStatus
    }

    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] {
        if let polls = closingPolls {
            if polls == 0 { sheetOpen = false; closingPolls = nil } else { closingPolls = polls - 1 }
        }
        return Dictionary(uniqueKeysWithValues: identifiers.filter(shown).map { ($0, "완료") })
    }

    func expand(_ identifier: String, timeout: TimeInterval) throws { try require(identifier) }
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws { throw T17Blocker(code: "ui-element-missing", detail: "no text injection") }
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws { throw T17Blocker(code: "ui-element-missing", detail: "no text injection") }
    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval) throws { try require("display") }
    func textSnapshot() -> [String] { [] }
}
