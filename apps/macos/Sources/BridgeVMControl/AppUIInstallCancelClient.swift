#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Foundation
import Darwin

@MainActor
final class AppUIInstallCancelClient {
    let host: AppUIHost
    let window: NSWindow
    let content: NSView
    let mailbox: AppUIInstallCancelMailbox
    private var admission: AppUIInstallCancelAdmission
    private(set) var lastReadSequence: UInt32 = 0
    private(set) var lastReply: AppUIInstallCancelReply?
    private var now: Double { ProcessInfo.processInfo.systemUptime }

    private init(host: AppUIHost, window: NSWindow, content: NSView,
                 mailbox: AppUIInstallCancelMailbox, admission: AppUIInstallCancelAdmission) {
        self.host = host; self.window = window; self.content = content
        self.mailbox = mailbox; self.admission = admission
    }

    static func connect(_ host: AppUIHost, window: NSWindow, content: NSView,
                        deadline: Double) async throws -> AppUIInstallCancelClient {
        let root = host.capture.output.deletingLastPathComponent()
        let mailbox = try AppUIInstallCancelMailbox(privateRoot: root)
        while ProcessInfo.processInfo.systemUptime < deadline {
            try checkOwner(host, window: window, content: content, mailbox: mailbox)
            if let record = try mailbox.readSession() {
                let session = record.session
                try verifySelf(session.host, privateRoot: root)
                let now = ProcessInfo.processInfo.systemUptime
                guard now < deadline else { throw AppUIDriverFailure.deadlineExceeded }
                let admission = try AppUIInstallCancelAdmission(session: session, sessionSHA256: record.sha256, now: now)
                if let ready = try mailbox.readReady() {
                    try AppUIInstallCancelValidation.ready(ready, session: session, sessionSHA256: record.sha256)
                    guard ready.trusted else { throw AppUIDriverFailure.accessibilityUntrusted }
                    try checkOwner(host, window: window, content: content, mailbox: mailbox)
                    return AppUIInstallCancelClient(host: host, window: window, content: content,
                                                 mailbox: mailbox, admission: admission)
                }
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw AppUIDriverFailure.deadlineExceeded
    }

    private static func verifySelf(_ identity: AppUIDriverProcessIdentity, privateRoot: URL) throws {
        let current = NSRunningApplication.current
        let bundle = Bundle.main.bundleURL.resolvingSymlinksInPath()
        guard identity.pid == getpid(), let launch = current.launchDate,
              identity.launchDate == launch.timeIntervalSince1970,
              identity.bundleIdentifier == Bundle.main.bundleIdentifier,
              identity.bundlePath == bundle.path,
              bundle.path == privateRoot.appendingPathComponent("BridgeVMAppUIHost.app").path,
              let executable = Bundle.main.executableURL?.resolvingSymlinksInPath(),
              identity.executablePath == executable.path,
              try AppUIHostCapture.digest(executable) == identity.executableSHA256 else {
            throw AppUIDriverFailure.identityMismatch
        }
    }

    private static func checkOwner(_ host: AppUIHost, window: NSWindow, content: NSView,
                                   mailbox: AppUIInstallCancelMailbox) throws {
        try Task.checkCancellation()
        try AppUIHost.checkCancellation(output: host.capture.output)
        guard AppUIHost.prepared === host, window.isVisible, window.contentView === content,
              content.window === window else { throw AppUIDriverFailure.identityMismatch }
        guard try !mailbox.isCancelled() else { throw AppUIDriverFailure.cancelled }
    }

    @discardableResult
    func perform(_ operation: AppUIInstallCancelOperation, phase: AppUIInstallCancelPhase,
                 deadline: Double) async throws -> AppUIInstallCancelValues? {
        try Self.checkOwner(host, window: window, content: content, mailbox: mailbox)
        guard now < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        let session = admission.session
        let request = AppUIInstallCancelRequest(sessionSHA256: admission.sessionSHA256,
            nonce: session.nonce, sequence: admission.nextSequence, phase: phase,
            phaseDeadlineUptime: deadline, window: AppUIInstallCancelOperationPolicy.window(for: operation), operation: operation)
        try admission.admit(request, now: now)
        _ = try mailbox.publishRequest(request)
        while now < deadline && now < session.deadlineUptime {
            try Self.checkOwner(host, window: window, content: content, mailbox: mailbox)
            if let reply = try mailbox.readReply(sequence: request.sequence) {
                try admission.complete(reply, now: now)
                if let failure = reply.failureCode { throw failure }
                lastReply = reply
                if operation == .readState { lastReadSequence = reply.sequence }
                return reply.values
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw AppUIDriverFailure.deadlineExceeded
    }

    static func wait(_ reason: String, deadline: Double,
                     until condition: () async throws -> Bool) async throws {
        while ProcessInfo.processInfo.systemUptime < deadline {
            try Task.checkCancellation()
            if try await condition() {
                guard ProcessInfo.processInfo.systemUptime < deadline else { break }
                return
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw AppUIHostError.refused("Timed out waiting for actual UI: \(reason)")
    }
}
#endif
