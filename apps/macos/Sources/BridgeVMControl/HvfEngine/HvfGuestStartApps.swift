import Foundation

/// One Windows app from the guest's Start menu (bvagent.ps1 Get-BvStartApps): its name and the AppID STARTAPP
/// launches.
struct HvfGuestStartApp: Equatable, Hashable, Identifiable {
    let name: String
    let appID: String
    var id: String { appID }

    init(name: String, appID: String) {
        self.name = name
        self.appID = appID
    }

    /// One reply row, `<b64(utf8 name)>|<b64(utf8 AppID)>`. Guest bytes: strict base64 and UTF-8, the name without
    /// control or format characters and at most 256 characters, and an AppID the agent's STARTAPP takes; else nil.
    init?(row: String) {
        guard row.utf8.count <= HvfGuestStartApps.maximumRowBytes else { return nil }
        let fields = row.split(separator: "|", omittingEmptySubsequences: false)
        guard fields.count == 2,
              let nameData = Data(base64Encoded: String(fields[0])), let idData = Data(base64Encoded: String(fields[1])),
              let rawName = String(data: nameData, encoding: .utf8), let appID = String(data: idData, encoding: .utf8),
              HvfGuestStartApps.isAppID(appID) else { return nil }
        let name = String(String.UnicodeScalarView(rawName.unicodeScalars.filter(HvfGuestStartApps.isShown)))
            .trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, name.count <= 256 else { return nil }
        self.init(name: name, appID: appID)
    }
}

/// 가상 머신 > Windows 앱: the guest's Start menu apps, launched in Windows. An agent whose READY carries `startapps`
/// answers `STARTAPPS <id>` with the list and `STARTAPP <b64(AppID)>` by starting the app through the user's shell
/// (bvagent.ps1); an older agent is never asked.
enum HvfGuestStartApps {
    static let readyMarker = "startapps"
    /// The agent sends at most 2000 apps in 512 KB; anything past these is dropped.
    static let maximumApps = 2000
    static let maximumBytes = 1024 * 1024
    static let maximumRowBytes = 8192
    /// The menu shows this many; 모든 앱… lists them all.
    static let menuLimit = 40

    static let notRunning = "VM이 실행 중일 때 사용할 수 있습니다."
    static let notReady = "Windows가 준비되면 사용할 수 있습니다."
    static let paused = "VM을 재개하면 사용할 수 있습니다."
    static let agentTooOld = "이 VM의 BridgeVM 게스트 도구가 업데이트되면 사용할 수 있습니다."
    static let notLoaded = "목록을 불러오려면 ‘목록 새로 고침’을 선택하세요."
    static let loading = "불러오는 중…"
    static let empty = "Windows 시작 메뉴에 앱이 없습니다."
    static let listUnavailable = "Windows가 시작 메뉴의 앱 목록을 알려 주지 않았습니다. 잠시 뒤 다시 시도하세요."
    static let noAnswer = "Windows가 앱 목록 요청에 답하지 않았습니다. 잠시 뒤 다시 시도하세요."
    static let notSent = "Windows에 요청을 보내지 못했습니다. 잠시 뒤 다시 시도하세요."
    static func launchFailed(_ name: String) -> String { "Windows에서 ‘\(name)’을(를) 열지 못했습니다." }

    /// Whether a READY line's text (`<host> v3-share2 ... startapps agent=<id>`) has the marker.
    static func advertised(_ ready: String) -> Bool {
        ready.split(separator: " ").contains { $0 == readyMarker }
    }

    /// Why the menu lists nothing, in one plain sentence; nil when the guest takes the verbs now.
    @MainActor static func unavailableReason(_ session: HvfEngineSession?) -> String? {
        guard let session else { return notRunning }
        switch session.connectionState {
        case .stopped, .stopping, .timedOut: return notRunning
        case .booting: return notReady
        case .connected:
            guard session.serviceStarted else { return notReady }
            if session.pause.holdsInput { return paused }
            return session.guestStartApps ? nil : agentTooOld
        }
    }

    /// The agent's own rule (bvagent.ps1 Test-BvStartAppId): an AUMID, a registered AppUserModelID or a known
    /// folder's `{GUID}\path\x.exe`, of letters, digits, spaces and `._+-()[]{}~#@!\`, without `..` or `\\`, no
    /// space or backslash at either end, at most 1024 UTF-16 units.
    static func isAppID(_ id: String) -> Bool {
        guard (1...1024).contains(id.utf16.count), !id.contains(".."), !id.contains("\\\\"),
              !id.hasPrefix(" "), !id.hasSuffix(" "), !id.hasPrefix("\\"), !id.hasSuffix("\\") else { return false }
        return id.unicodeScalars.allSatisfy { scalar in
            switch scalar.properties.generalCategory {
            case .uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter,
                 .nonspacingMark, .spacingMark, .enclosingMark, .decimalNumber, .letterNumber, .otherNumber:
                return true
            default:
                return " ._+()[]{}~#@!\\-".unicodeScalars.contains(scalar)
            }
        }
    }

    static func launchCommand(_ appID: String) -> String? {
        isAppID(appID) ? "STARTAPP " + Data(appID.utf8).base64EncodedString() : nil
    }

    static func isShown(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.properties.generalCategory {
        case .control, .format, .lineSeparator, .paragraphSeparator, .surrogate, .unassigned: return false
        default: return true
        }
    }

    static func sorted(_ apps: [HvfGuestStartApp]) -> [HvfGuestStartApp] {
        apps.sorted {
            let order = $0.name.localizedStandardCompare($1.name)
            return order == .orderedSame ? $0.appID < $1.appID : order == .orderedAscending
        }
    }

    /// The menu's entries: at most `menuLimit`, apps before Windows' own tools, documents and uninstallers,
    /// shown alphabetically.
    static func menuApps(_ apps: [HvfGuestStartApp]) -> [HvfGuestStartApp] {
        let all = sorted(apps)
        return sorted(Array((all.filter { !isSecondary($0) } + all.filter(isSecondary)).prefix(menuLimit)))
    }

    /// FOLDERID_System, FOLDERID_SystemX86 and FOLDERID_Windows: Windows' own tools (Command Prompt, Character Map).
    private static let windowsFolders = ["{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\", "{D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27}\\",
                                         "{F38BF404-1D43-42F2-9305-67DE0B28FC23}\\"]

    private static func isSecondary(_ app: HvfGuestStartApp) -> Bool {
        let id = app.appID.uppercased(), name = app.name.lowercased()
        if windowsFolders.contains(where: { id.hasPrefix($0) }) || (id.contains("\\") && !id.hasSuffix(".EXE")) { return true }
        return ["uninstall", "제거", "readme", "documentation", "release notes"].contains { name.contains($0) }
    }

    static func restarts(_ line: String) -> Bool {
        line.hasPrefix("BVAGENT READY") || line.hasPrefix("BVAGENT re-READY") || line.hasPrefix("BVAGENT SERVICE start")
            || HvfGuestResetRecord.matches(line)
    }
}

/// One `STARTAPPS <id>` question. The relay logs the agent's OUT reply as `BVAGENT CMD <command> exit=<n>`, its rows
/// and `BVAGENT END <command>`, so only the frame carrying this ID answers it. Rows that do not parse are skipped;
/// past `maximumApps` or `maximumBytes` the rest are dropped.
struct HvfGuestStartAppsRequest {
    enum Failure: Error, Equatable { case unavailable, restarted, expired }
    static let timeout: TimeInterval = 30
    let command: String
    let deadline: Date
    private var answering = false
    private var apps: [HvfGuestStartApp] = []
    private var seen: Set<String> = []
    private var bytes = 0

    init(now: Date, id: UUID = UUID()) {
        command = "STARTAPPS " + id.uuidString
        deadline = now.addingTimeInterval(Self.timeout)
    }

    /// nil while unanswered and before the deadline; the lines are read before the deadline is checked.
    mutating func consume(lines: [String], now: Date) -> Result<[HvfGuestStartApp], Failure>? {
        for raw in lines {
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            if HvfGuestStartApps.restarts(line) { return .failure(.restarted) }
            if answering {
                if line == "BVAGENT END \(command)" { return .success(HvfGuestStartApps.sorted(apps)) }
                bytes += line.utf8.count
                guard apps.count < HvfGuestStartApps.maximumApps, bytes <= HvfGuestStartApps.maximumBytes,
                      let app = HvfGuestStartApp(row: line), seen.insert(app.appID.lowercased()).inserted else { continue }
                apps.append(app)
            } else if line.hasPrefix("BVAGENT CMD \(command) exit=") {
                guard line == "BVAGENT CMD \(command) exit=0" else { return .failure(.unavailable) }
                answering = true
            }
        }
        return now < deadline ? nil : .failure(.expired)
    }
}

/// One `STARTAPP <b64(AppID)>`: `exit=0` is opened, another exit refused; a guest restart or no answer within
/// `timeout` ends it without a word (the app may still have opened).
struct HvfGuestStartAppLaunch {
    enum Answer: Equatable { case opened, refused, gone }
    static let timeout: TimeInterval = 30
    let app: HvfGuestStartApp
    let command: String
    let deadline: Date

    init?(app: HvfGuestStartApp, now: Date) {
        guard let command = HvfGuestStartApps.launchCommand(app.appID) else { return nil }
        self.app = app
        self.command = command
        deadline = now.addingTimeInterval(Self.timeout)
    }

    func consume(lines: [String], now: Date) -> Answer? {
        for raw in lines {
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            if HvfGuestStartApps.restarts(line) { return .gone }
            if line.hasPrefix("BVAGENT CMD \(command) exit=") { return line == "BVAGENT CMD \(command) exit=0" ? .opened : .refused }
        }
        return now < deadline ? nil : .gone
    }
}
