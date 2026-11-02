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
            return isBlocked ? ("공유 폴더를 확인할 수 없어 일부 동기화를 기다리고 있습니다.",
                                "원래 폴더의 연결을 확인하세요. 다른 폴더를 쓰려면 VM을 종료한 뒤 다시 선택하세요.") : nil
        }
    }
}
