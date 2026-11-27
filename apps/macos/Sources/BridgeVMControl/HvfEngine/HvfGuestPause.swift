import Foundation

/// Pause and Resume for one VM. input.ctl `PAUSE` parks every vCPU and holds the devices' clocks;
/// `RESUME` runs them again, with guest time continuing where it stopped (the probe's pause_gate).
/// The probe answers on run.log (`BVPAUSE paused`, `BVPAUSE resumed …`, `BVPAUSE refused: …`), and
/// the session shows what it last said, so a reattached app reads the helper's real state too.
/// A full Mac disk pauses the VM on its own (`BVSTORAGE full …` first): Windows' writes wait instead
/// of failing, and a Resume while the disk is still full is answered paused again.
enum HvfPauseState: Equatable {
    case running
    /// `PAUSE` sent at `sentAt` (`ownedStartNow`); the probe has not answered yet.
    case pausing(sentAt: TimeInterval)
    case paused
    /// `RESUME` sent at `sentAt`; the probe has not answered yet.
    case resuming(sentAt: TimeInterval)

    /// Guest input is ignored from the press of Pause until the guest runs again.
    var holdsInput: Bool { self != .running }
    /// The display shows "▶ Resume" and Resume is offered.
    var showsResume: Bool {
        switch self {
        case .pausing, .paused: return true
        case .running, .resuming: return false
        }
    }
}

/// Host uptime with the paused stretches left out, as the probe's vm_clock is for the devices. The app's
/// deadlines for guest answers (input receipts and the release cleanup, window inventory, guest
/// commands) run on it, so a pause never uses up the time the guest has to answer.
struct HvfGuestRunClock {
    private var excluded: TimeInterval = 0
    private var heldSince: Date?

    func now(host: Date) -> Date { (heldSince ?? host).addingTimeInterval(-excluded) }

    /// Held from the press of Pause until the guest runs again (`HvfPauseState.holdsInput`).
    mutating func hold(_ held: Bool, host: Date) {
        if held {
            if heldSince == nil { heldSince = host }
        } else if let since = heldSince {
            excluded += max(0, host.timeIntervalSince(since))
            heldSince = nil
        }
    }
}

enum HvfPauseOutcome: Equatable {
    case requested
    case refused(String)

    var message: String? {
        if case let .refused(text) = self { return text }
        return nil
    }
}

extension HvfEngineSession {
    static let pauseCommand = "PAUSE"
    static let resumeCommand = "RESUME"
    /// The probe parks every vCPU within 2 s (`PARK_TIMEOUT`) or refuses; one it never answers (a
    /// helper without Pause, or one that ended) is dropped after this long.
    static let pauseAnswerTimeout: TimeInterval = 5
    static let diskFullNotice = "Windows is paused because your Mac’s disk is full. Free up space on your Mac, then choose Resume."

    /// The clock for the app's deadlines on guest answers (HvfGuestRunClock).
    func guestNow() -> Date { guestRunClock.now(host: hostNow()) }

    var pauseAvailable: Bool {
        vmmControlAvailable && !runtimeStartupWorkerPending
    }

    /// Pause, or Resume when paused: the toolbar's and the menu's one button.
    @discardableResult
    func togglePause() -> HvfPauseOutcome {
        pause.showsResume ? requestResume() : requestPause()
    }

    func requestPause() -> HvfPauseOutcome {
        guard pause == .running else { return .refused("The VM is already pausing or paused.") }
        guard pauseAvailable else { return .refused("Only a running VM can be paused.") }
        // Held keys and buttons go up first, as when the window loses focus.
        cancelOrderedInputTarget()
        guard appendVMMControl(Self.pauseCommand) else {
            return .refused("Couldn’t send the pause request to the VM.")
        }
        pause = .pausing(sentAt: ownedStartNow())
        pauseNotice = nil
        append(.unknown("pause requested"))
        return .requested
    }

    func requestResume() -> HvfPauseOutcome {
        switch pause {
        case .running: return .refused("The VM is already running.")
        case .resuming: return .requested
        case .pausing, .paused:
            guard appendRunControl(Self.resumeCommand) else {
                return .refused("Couldn’t send the resume request to the VM.")
            }
            pause = .resuming(sentAt: ownedStartNow())
            pauseNotice = nil
            return .requested
        }
    }

    /// Stop, Force Stop, quit and Restart: a paused Windows cannot shut down or restart, so it runs
    /// first. (The probe also resumes on its own before a host stop, SIGTERM included.)
    func resumeBeforeStop() {
        guard pause.showsResume else { return }
        _ = requestResume()
    }

    /// An app that attaches to a running helper reads its run.log from the start, so it knows when
    /// the app that started the helper left it paused: the VM runs again. A helper that is not
    /// paused gets nothing.
    func resumeAfterReattach() {
        guard pause.showsResume else { return }
        _ = requestResume()
    }

    /// A `BVPAUSE` line from run.log.
    func pauseReported(_ line: String) {
        let words = line.split(separator: " ", maxSplits: 2)
        guard words.count >= 2, words[0] == "BVPAUSE" else { return }
        switch words[1] {
        case "paused" where diskFullReported: // also the answer to a RESUME while the disk is still full
            diskFullReported = false
            pause = .paused
            pauseNotice = Self.diskFullNotice
        case "paused":
            if case .resuming = pause { return } // a RESUME already went after it
            pause = .paused
        case "resumed":
            pause = .running
        case "refused:":
            diskFullReported = false
            pause = .running
            pauseNotice = "The VM couldn’t pause and is still running."
        default:
            return
        }
    }

    /// `BVSTORAGE full errno=… free=… path=…`: the probe pauses the VM for a full Mac disk next.
    func storageReported(_ line: String) {
        if line.hasPrefix("BVSTORAGE full ") { diskFullReported = true }
    }

    /// A guest reset or a run's start or end: nothing is paused any more.
    func pauseEnded() {
        diskFullReported = false
        if pause != .running { pause = .running }
        if pauseNotice != nil { pauseNotice = nil }
    }

    /// A `PAUSE` or `RESUME` nothing answered: the last state the probe reported stands.
    func expireUnansweredPause() {
        switch pause {
        case let .pausing(sentAt) where ownedStartNow() - sentAt >= Self.pauseAnswerTimeout:
            pause = .running
            pauseNotice = "The VM didn’t respond to the pause request and is still running."
        case let .resuming(sentAt) where ownedStartNow() - sentAt >= Self.pauseAnswerTimeout:
            pause = .paused
            pauseNotice = "The VM didn’t respond to the resume request. Try Resume again."
        default:
            break
        }
    }
}
