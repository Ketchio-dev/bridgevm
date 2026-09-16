import AppKit
import Foundation
import Darwin

@MainActor
final class AppUIHostV2Supervisor {
    let paths: AppUIHostV2Paths
    let startedAt: Double
    private var owners: [AppUIHostV2Role: AppUIHostV2Ownership]
    private var applications: [AppUIHostV2Role: NSRunningApplication] = [:]
    private var observer: NSObjectProtocol?
    private var signals: [DispatchSourceSignal] = []
    private var mailbox: AppUIDriverMailbox?
    private var sessionPublished = false
    private var scenarioVerified = false
    private var nonce: String?
    private var lastReceipt: Data?
    private var failure: String?
    private var cancelled = false
    private var timedOut = false
    private var now: Double { ProcessInfo.processInfo.systemUptime }

    init(paths: AppUIHostV2Paths, startedAt: Double) {
        self.paths = paths; self.startedAt = startedAt
        let date = Date().timeIntervalSince1970
        owners = Dictionary(uniqueKeysWithValues: AppUIHostV2Role.allCases.map {
            ($0, AppUIHostV2Ownership(role: $0, bundlePath: paths.bundle($0).path,
                                     executableSHA256: paths.digest($0), requestedAt: date))
        })
    }

    func run() -> Int32 {
        installSignals()
        defer {
            if let observer { NSWorkspace.shared.notificationCenter.removeObserver(observer) }
            signals.forEach { $0.cancel() }
        }
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didLaunchApplicationNotification, object: nil, queue: nil
        ) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
            DispatchQueue.main.async { self?.observeNotification(app) }
        }
        do {
            nonce = try AppUIDriverValidation.makeNonce()
            try AppUIDriverMailbox.prepare(privateRoot: paths.root)
            mailbox = try AppUIDriverMailbox(privateRoot: paths.root)
        } catch { stop("Driver session preparation failed") }
        checkStop()
        save()
        if failure == nil {
            launch(.host)
            launch(.driver)
        }
        while !owners.values.allSatisfy(\.finished) {
            checkStop()
            for role in AppUIHostV2Role.allCases {
                guard var owner = owners[role] else { continue }
                let action = owner.poll(now: now, exited: applications[role]?.isTerminated == true)
                owners[role] = owner
                if action == .terminate || action == .kill { signal(role, action: action) }
            }
            publishSessionIfReady()
            save()
            if !owners.values.allSatisfy(\.finished) {
                _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
            }
        }
        verifyScenarioCompletion()
        save()
        return failure == nil && scenarioVerified && owners.values.allSatisfy(\.success) ? 0 : 2
    }

    private func launch(_ role: AppUIHostV2Role) {
        guard failure == nil, var owner = owners[role] else { return }
        owner.willLaunch(); owners[role] = owner
        save()
        // If saving failed, willLaunch remains true: uncertain launch never becomes no-launch proof.
        guard failure == nil else { return }
        NSWorkspace.shared.openApplication(at: paths.bundle(role),
            configuration: AppUIHostV2Application.configuration(role, paths: paths)) { [weak self] app, _ in
            DispatchQueue.main.async {
                guard let self else { return }
                guard let app else { self.stop("NSWorkspace did not return the requested application"); return }
                self.observe(app, role: role)
            }
        }
    }

    private func observeNotification(_ app: NSRunningApplication) {
        for role in AppUIHostV2Role.allCases where app.bundleURL?.resolvingSymlinksInPath().path == paths.bundle(role).path {
            observe(app, role: role)
        }
    }

    private func observe(_ app: NSRunningApplication, role: AppUIHostV2Role) {
        guard var owner = owners[role] else { return }
        do {
            let identity = try AppUIHostV2Application.identity(app, expected: owner,
                allowTerminatedObservation: owner.finished)
            if owner.observe(identity, now: now) { applications[role] = app }
            owners[role] = owner
            if owner.failure != nil { stop("Launched application identity conflict") }
        } catch {
            owner.refuseObservation("Could not verify the launched application identity", now: now)
            owners[role] = owner
            stop("Could not verify the launched application identity")
        }
        publishSessionIfReady()
        save()
    }

    private func publishSessionIfReady() {
        guard !sessionPublished, failure == nil, let nonce, let mailbox,
              let host = owners[.host]?.identity, let driver = owners[.driver]?.identity else { return }
        do {
            _ = try mailbox.publishSession(AppUIDriverSession(nonce: nonce, startedUptime: startedAt,
                deadlineUptime: startedAt + 90, host: host, driver: driver))
            sessionPublished = true
        } catch { stop("Could not publish the admitted driver session") }
    }

    private func verifyScenarioCompletion() {
        guard failure == nil, owners.values.allSatisfy(\.success) else { return }
        do {
            guard let mailbox, let driver = try mailbox.readCompletion(),
                  let host = try mailbox.readHostCompletion(), host.success, driver.success else {
                failure = "The owned applications exited without a successful complete scenario"
                return
            }
            scenarioVerified = true
        } catch { failure = "The owned application completion records could not be verified" }
    }

    private func checkStop() {
        if now >= startedAt + 90 {
            timedOut = true
            stop("Native app diagnostic exceeded its 90 second deadline")
        } else if AppUIHostLaunchPaths.exists(paths.cancellation.path), failure == nil {
            cancelled = true
            stop("Owning job cancelled the native app diagnostic")
        }
    }

    private func stop(_ reason: String) {
        failure = failure ?? reason
        for role in AppUIHostV2Role.allCases {
            owners[role]?.requestStop(reason, now: now)
        }
        if !AppUIHostLaunchPaths.exists(paths.cancellation.path) {
            do { try Data("cancel\n".utf8).write(to: paths.cancellation, options: .withoutOverwriting) }
            catch { failure = "Cancellation marker could not be published" }
        }
    }

    private func signal(_ role: AppUIHostV2Role, action: AppUIHostV2Ownership.Action) {
        guard var owner = owners[role] else { return }
        if applications[role]?.isTerminated == true {
            _ = owner.poll(now: now, exited: true); owners[role] = owner; return
        }
        let current = owner.identity.flatMap { NSRunningApplication(processIdentifier: $0.pid) }
            .flatMap { try? AppUIHostV2Application.identity($0, expected: owner) }
        if current == nil && applications[role]?.isTerminated == true {
            _ = owner.poll(now: now, exited: true); owners[role] = owner; return
        }
        let pid = owner.authorize(action, current: current, now: now)
        owners[role] = owner
        guard let pid else { stop("Refused to signal an unverified application"); return }
        if Darwin.kill(pid, action == .terminate ? SIGTERM : SIGKILL) != 0 && errno != ESRCH {
            stop("Verified application signal failed")
        }
    }

    private func installSignals() {
        for number in [SIGTERM, SIGINT] {
            Darwin.signal(number, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: number, queue: .main)
            source.setEventHandler { [weak self] in
                DispatchQueue.main.async { self?.cancelled = true; self?.stop("Owning job cancelled the native app diagnostic") }
            }
            source.resume(); signals.append(source)
        }
    }

    private func save() {
        guard let host = owners[.host], let driver = owners[.driver] else { return }
        let receipt: [String: Any] = ["schema_version": 2, "kind": "native-app-ui-launcher-v2",
            "started_uptime": startedAt, "deadline_uptime": startedAt + 90,
            "host": host.receipt, "driver": driver.receipt,
            "cleanup_verified": host.cleanupVerified && driver.cleanupVerified,
            "success": failure == nil && scenarioVerified && host.success && driver.success,
            "cancelled": cancelled, "timed_out": timedOut,
            "failure": failure.map { $0 as Any } ?? NSNull()]
        do {
            let data = try JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys])
            guard data != lastReceipt else { return }
            try data.write(to: paths.receipt, options: lastReceipt == nil ? .withoutOverwriting : .atomic)
            lastReceipt = data
        } catch { stop("Launcher receipt could not be saved") }
    }
}
