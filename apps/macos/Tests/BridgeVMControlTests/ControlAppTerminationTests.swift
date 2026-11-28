import AppKit
import Foundation
import XCTest
@testable import BridgeVMControl

/// Quitting stops each running Windows first and answers AppKit only after every teardown.
@MainActor
final class ControlAppTerminationTests: XCTestCase {
    @MainActor final class FakeSession: ControlAppTerminationSession {
        var blocksAppTermination: Bool
        var terminationName: String?
        var stops = 0, forces = 0
        init(running: Bool, name: String? = nil) { blocksAppTermination = running; terminationName = name }
        func requestTerminationStop() { stops += 1 }
        func requestTerminationForceStop() { forces += 1 }
        func pollTerminationWait() {}
    }

    @MainActor final class Harness {
        var sessions: [FakeSession] = []
        var confirmations: [Int] = [], confirmAnswer = true
        var deadlines: [Bool] = [], deadlineAnswers: [ControlAppTermination.DeadlineChoice] = []
        var deadlineNames: [[String]] = []
        var replies: [Bool] = []
        var clock: TimeInterval = 100
        lazy var termination = ControlAppTermination(
            sessions: { [unowned self] in self.sessions },
            presenter: .init(
                confirmQuit: { [unowned self] count in self.confirmations.append(count); return self.confirmAnswer },
                decideAtDeadline: { [unowned self] afterForce, names, _ in
                    self.deadlines.append(afterForce); self.deadlineNames.append(names)
                    return self.deadlineAnswers.removeFirst()
                }),
            reply: { [unowned self] in self.replies.append($0) }, now: { [unowned self] in self.clock })
    }

    func testRunningSessionsTerminateLaterWithOneStopEachAndReplyAfterTeardown() {
        let h = Harness()
        let first = FakeSession(running: true), second = FakeSession(running: true), idle = FakeSession(running: false)
        h.sessions = [first, idle, second]

        XCTAssertEqual(h.termination.shouldTerminate(reason: .user), .terminateLater)
        XCTAssertEqual(h.confirmations, [2], "One alert for every running VM")
        XCTAssertEqual([first.stops, second.stops, idle.stops], [1, 1, 0])
        first.blocksAppTermination = false
        h.termination.poll()
        XCTAssertEqual(h.replies, [], "The owner lock stays until every stop completes")
        second.blocksAppTermination = false
        h.termination.poll()
        XCTAssertEqual(h.replies, [true])
        h.termination.poll()
        XCTAssertEqual(h.replies, [true], "AppKit gets one answer")
    }

    func testNoRunningSessionTerminatesNowWithoutAlert() {
        let h = Harness()
        h.sessions = [FakeSession(running: false)]
        XCTAssertEqual(h.termination.shouldTerminate(reason: .user), .terminateNow)
        XCTAssertEqual(h.termination.shouldTerminate(reason: .logoutRestartOrShutdown), .terminateNow)
        XCTAssertEqual(h.confirmations, [])
        XCTAssertEqual(h.sessions[0].stops, 0)
    }

    func testLogoutRestartOrShutdownStopsWithoutAlert() {
        let h = Harness()
        let running = FakeSession(running: true)
        h.sessions = [running]
        XCTAssertEqual(h.termination.shouldTerminate(reason: .logoutRestartOrShutdown), .terminateLater)
        XCTAssertEqual(h.confirmations, [])
        XCTAssertEqual(running.stops, 1)
    }

    func testCancelledAlertLeavesTheVMRunning() {
        let h = Harness()
        let running = FakeSession(running: true)
        h.sessions = [running]
        h.confirmAnswer = false
        XCTAssertEqual(h.termination.shouldTerminate(reason: .user), .terminateCancel)
        XCTAssertEqual(running.stops, 0)
    }

    func testShutdownDelayDefaultsToKeepWaitingBeforeAndAfterAForceStop() {
        for afterForce in [false, true] {
            let alert = ControlAppTermination.Presenter.shutdownDelayAlert(afterForce: afterForce)
            alert.layout()
            XCTAssertEqual(alert.buttons.map(\.title), ["Keep Waiting", afterForce ? "Quit Anyway" : "Force Stop and Quit", "Cancel Quit"])
            XCTAssertEqual(alert.buttons[0].keyEquivalent, "\r")
            XCTAssertTrue(alert.buttons[1].hasDestructiveAction)
            XCTAssertEqual(alert.buttons[2].keyEquivalent, "\u{1b}")
        }
    }

    /// With two VMs running, the deadline question says which one Windows is still shutting down in.
    func testDeadlineAlertNamesTheVMsStillShuttingDown() {
        let h = Harness()
        let work = FakeSession(running: true, name: "Work"), games = FakeSession(running: true, name: "Games")
        let install = FakeSession(running: true)
        h.sessions = [work, games, install]
        XCTAssertEqual(h.termination.shouldTerminate(reason: .logoutRestartOrShutdown), .terminateLater)
        work.blocksAppTermination = false
        h.deadlineAnswers = [.keepWaiting]
        h.clock += ControlAppTermination.deadline; h.termination.poll()
        XCTAssertEqual(h.deadlineNames, [["Games"]], "A VM that already stopped isn't named")

        typealias P = ControlAppTermination.Presenter
        XCTAssertEqual(P.shutdownDelayAlert(afterForce: false, names: ["Games"]).messageText,
                       "Windows is still shutting down in “Games”.")
        XCTAssertEqual(P.shutdownDelayAlert(afterForce: false, names: ["Work", "Games", "Dev"]).messageText,
                       "Windows is still shutting down in “Work”, “Games” and “Dev”.")
        XCTAssertEqual(P.shutdownDelayAlert(afterForce: true, names: ["Games"]).messageText, "“Games” is still stopping.")
        XCTAssertEqual(P.shutdownDelayAlert(afterForce: true, names: ["Work", "Games"]).messageText,
                       "“Work” and “Games” are still stopping.")
        XCTAssertEqual(P.shutdownDelayAlert(afterForce: false).messageText, "Windows is still shutting down.")
    }

    func testDeadlineOffersForceStopThenQuitAnyway() {
        let h = Harness()
        let running = FakeSession(running: true)
        h.sessions = [running]
        XCTAssertEqual(h.termination.shouldTerminate(reason: .logoutRestartOrShutdown), .terminateLater)
        h.clock += ControlAppTermination.deadline - 1; h.termination.poll()
        XCTAssertEqual(h.deadlines, [])
        h.deadlineAnswers = [.keepWaiting, .forceStop, .quitAnyway]
        h.clock += 1; h.termination.poll()
        XCTAssertEqual(h.deadlines, [false])
        XCTAssertEqual(running.forces, 0)
        h.clock += ControlAppTermination.deadline; h.termination.poll()
        XCTAssertEqual(h.deadlines, [false, false])
        XCTAssertEqual(running.forces, 1)
        XCTAssertEqual(h.replies, [])
        h.clock += ControlAppTermination.deadline; h.termination.poll()
        XCTAssertEqual(h.deadlines, [false, false, true], "After a Force Stop the user may quit anyway")
        XCTAssertEqual(h.replies, [true])
    }

    func testCancelAtTheDeadlineKeepsTheAppAndAllowsALaterQuit() {
        let h = Harness()
        let running = FakeSession(running: true)
        h.sessions = [running]
        XCTAssertEqual(h.termination.shouldTerminate(reason: .user), .terminateLater)
        h.deadlineAnswers = [.cancelQuit]
        h.clock += ControlAppTermination.deadline; h.termination.poll()
        XCTAssertEqual(h.replies, [false])
        XCTAssertEqual(h.termination.shouldTerminate(reason: .user), .terminateLater)
        XCTAssertEqual(h.confirmations, [1, 1])
    }

    func testQuitReasonComesFromTheQuitAppleEvent() {
        func quit(_ reason: OSType?, asParameter: Bool = false) -> NSAppleEventDescriptor {
            let event = NSAppleEventDescriptor(eventClass: AEEventClass(kCoreEventClass),
                eventID: AEEventID(kAEQuitApplication), targetDescriptor: nil,
                returnID: AEReturnID(kAutoGenerateReturnID), transactionID: AETransactionID(kAnyTransactionID))
            if let reason, asParameter {
                event.setParam(NSAppleEventDescriptor(enumCode: reason), forKeyword: AEKeyword(kAEQuitReason))
            } else if let reason {
                event.setAttribute(NSAppleEventDescriptor(typeCode: reason), forKeyword: AEKeyword(kAEQuitReason))
            }
            return event
        }
        XCTAssertEqual(ControlAppTermination.QuitReason.current(nil), .user, "Cmd+Q sends no Apple Event")
        XCTAssertEqual(ControlAppTermination.QuitReason.current(quit(nil)), .user, "The Dock's Quit has no reason")
        for reason in [kAEReallyLogOut, kAERestart, kAEShutDown] {
            XCTAssertEqual(ControlAppTermination.QuitReason.current(quit(OSType(reason))), .logoutRestartOrShutdown)
            XCTAssertEqual(ControlAppTermination.QuitReason.current(quit(OSType(reason), asParameter: true)),
                           .logoutRestartOrShutdown)
        }
    }
}
