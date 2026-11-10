/// The runtime stops sharing for the rest of a boot generation on these exact lines; a stop
/// outranks a root deferral in the notice. Wording promises nothing beyond this run.
enum HvfShareStop: Equatable {
    case agentTooOld, failedDestinations

    init?(line: String) {
        switch line {
        case "BVAGENT SHARE agent-too-old (needs v3-share2); share disabled": self = .agentTooOld
        case "BVAGENT SHARE disabled failed-destination-limit": self = .failedDestinations
        default: return nil
        }
    }
}

extension HvfShareRootNotice {
    var displayed: (text: String, help: String)? {
        switch stopped {
        case .agentTooOld:
            return ("이 VM의 게스트 에이전트가 공유 폴더 동기화를 지원하지 않아 이번 실행에서는 동기화하지 않습니다.",
                    "공유 폴더 없이 VM은 계속 실행됩니다.")
        case .failedDestinations:
            return ("Mac 폴더에 쓰기가 반복해서 실패해 이번 실행에서 공유 폴더 동기화를 중지했습니다.",
                    "폴더 권한과 남은 공간을 확인한 뒤 VM을 다시 시작하세요.")
        case nil:
            if isBlocked {
                return ("공유 폴더를 확인할 수 없어 일부 동기화를 기다리고 있습니다.",
                        "원래 폴더의 연결을 확인하세요. 다른 폴더를 쓰려면 VM을 종료한 뒤 다시 선택하세요.")
            }
            if let lowSpace {
                let files = lowSpace.count == 1 ? "'\(lowSpace.last)'을(를)" : "'\(lowSpace.last)' 외 \(lowSpace.count - 1)개를"
                return ("Mac 디스크 공간이 부족해 \(files) 공유 폴더로 가져오지 않았습니다.",
                        "공간을 확보하면 다시 가져옵니다.")
            }
            guard let tooLarge else { return nil }
            let kib = HvfEngineConfig.shareMaxKiB
            let limit = kib % (1024 * 1024) == 0 ? "\(kib / (1024 * 1024)) GB" : "\(kib / 1024) MB"
            let files = tooLarge.count == 1 ? tooLarge.last : "\(tooLarge.last) 외 \(tooLarge.count - 1)개"
            return ("\(limit)보다 큰 파일은 공유 폴더로 옮기지 않습니다: \(files)",
                    "Mac의 파일은 VM 창에 끌어다 놓으면 Windows 바탕 화면으로 옮겨집니다.")
        }
    }
}

/// A shared-folder file left out for its size (`BVAGENT SHARE skip <name> too-large <bytes>`).
struct HvfShareTooLarge: Equatable {
    let count: Int
    let last: String

    static func name(in line: String) -> String? { hvfShareSkippedName(in: line, reason: "too-large") }
}

/// Windows files the Mac had no room for and that have not reached it since
/// (`BVAGENT SHARE skip <name> low-space <bytes>`): a large file needs its size plus 2 GiB free,
/// and any write the Mac refuses for want of space is reported too. The runtime asks again at each
/// guest listing and reports each version once; a file counts once until it arrives.
struct HvfShareLowSpace: Equatable {
    /// Each waiting file, and when it was last reported (a running number).
    private var waiting: [String: Int]
    private var reports = 0
    /// The most recently reported file still waiting.
    private(set) var last: String
    var count: Int { waiting.count }

    init(_ name: String) {
        waiting = [name: 0]
        last = name
    }

    static func name(in line: String) -> String? { hvfShareSkippedName(in: line, reason: "low-space") }

    func reported(_ name: String) -> Self {
        var next = self
        next.reports += 1
        next.waiting[name] = next.reports
        next.last = name
        return next
    }

    func isWaiting(for name: String) -> Bool { waiting[name] != nil }

    /// Without `name`, which reached the Mac; nil once no file is left waiting.
    func arrived(_ name: String) -> Self? {
        var next = self
        next.waiting[name] = nil
        guard !next.waiting.isEmpty else { return nil }
        if name == last, let newest = next.waiting.max(by: { $0.value < $1.value }) {
            next.last = newest.key
        }
        return next
    }
}

/// The name in `BVAGENT SHARE skip <name> <reason> <ASCII digits>`.
private func hvfShareSkippedName(in line: String, reason: String) -> String? {
    let prefix = "BVAGENT SHARE skip "
    guard line.hasPrefix(prefix) else { return nil }
    // Only after the prefix: a line that is all marker must not slice backwards.
    let rest = line.dropFirst(prefix.count)
    guard let marker = rest.range(of: " \(reason) ", options: .backwards) else { return nil }
    let size = rest[marker.upperBound...]
    guard !size.isEmpty, size.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 }) else { return nil }
    let name = rest[..<marker.lowerBound]
    return name.isEmpty ? nil : String(name)
}
