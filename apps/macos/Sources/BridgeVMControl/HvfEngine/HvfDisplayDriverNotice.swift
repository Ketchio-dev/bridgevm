import SwiftUI

/// Only the 2D launch needs this driver. Installation during a run takes effect at the next launch.
enum HvfDisplayDriverNotice: Equatable {
    case install, restart, unbound

    /// Optional protocol field: old agents and future values remain unknown.
    enum Virtio: String { case found, none }
    static func virtio(in line: String) -> Virtio? {
        let fields = line.split(whereSeparator: \.isWhitespace)
        guard fields.first == "BVDISPLAY" else { return nil }
        return fields.first(where: { $0.hasPrefix("virtio=") })
            .flatMap { Virtio(rawValue: String($0.dropFirst("virtio=".count))) }
    }

    /// `unbound`: the VM starts with the new display adapter, but Windows has no driver for it, as after
    /// restoring a snapshot taken before the driver was installed. Installing it again is the way back;
    /// the notice only pointed at Device Manager.
    var offersInstall: Bool { self != .restart }

    var text: String {
        switch self {
        case .unbound:
            return "Windows has no driver for the new display adapter, so the desktop doesn’t resize with this window. Choose Install Driver to install it again."
        case .install: return "Install the display driver to resize the Windows desktop when you resize this window."
        case .restart: return "To use the new display driver, choose Restart Now to shut down Windows and start the VM again."
        }
    }

    static func shown(libraryBacked: Bool, agentReady: Bool, installed: Bool?, experimental3D: Bool,
                      dismissed: Bool, restartRequired: Bool) -> Self? {
        guard libraryBacked, agentReady, installed != true, !experimental3D, !dismissed else { return nil }
        return restartRequired ? .restart : .install
    }
}

extension HvfEngineSession {
    var displayDriverNotice: HvfDisplayDriverNotice? {
        if displayDriverUnbound, config.libraryContext?.config.virtioDisplayDriverInstalled == true,
           !displayDriverUnboundDismissed { return .unbound }
        return HvfDisplayDriverNotice.shown(libraryBacked: config.libraryContext != nil,
            agentReady: HvfDisplayDriverInstall.agentReady(self),
            installed: config.libraryContext?.config.virtioDisplayDriverInstalled,
            experimental3D: config.virtioGpu3d, dismissed: displayDriverNoticeDismissed,
            restartRequired: displayDriverRestartRequired)
    }

    /// The latest known adapter status wins; recovery never resets this run's dismissal.
    func displayDriverReported(_ output: String) {
        guard config.libraryContext?.config.virtioDisplayDriverInstalled == true else { return }
        if let virtio = output.split(whereSeparator: \.isNewline).compactMap({
            HvfDisplayDriverNotice.virtio(in: String($0))
        }).last { displayDriverUnbound = virtio == .none }
    }

    /// A Windows restart cannot attach a device. Join the normal graceful Stop, wait for full
    /// cleanup and the saved driver flag, then use the normal new-runtime start path.
    @discardableResult
    func restartForDisplayDriver() -> Task<Void, Never>? {
        guard !displayDriverRestarting, terminationStopAvailable,
              let identity = ownedProcessIdentity else { return nil }
        // requestGUIStart refuses while a Settings sheet is open: Windows would shut down and stay off.
        guard openSettingsSheets.isEmpty else {
            displayDriverRestartNote = "Close Settings, then choose Restart Now."
            return nil
        }
        displayDriverRestartNote = nil
        displayDriverRestarting = true
        return Task { @MainActor in
            defer { displayDriverRestarting = false }
            guard case .requested = stopOwned(expectedToken: identity.token) else { return }
            while blocksAppTermination {
                try? await Task.sleep(nanoseconds: 250_000_000)
                pollTerminationWait()
            }
            guard lastOwnedExit?.identity == identity, connectionState == .stopped else { return }
            HvfDisplayDriverFlag.saveIfStopped(self)
            guard config.libraryContext?.config.virtioDisplayDriverInstalled == true else {
                displayDriverRestartNote = displayDriverFlagMessage
                    ?? "BridgeVM couldn’t save the display driver setting, so start Windows again from the library."
                return
            }
            if case let .refused(reason) = requestGUIStart(configuration: config, policy: .requireNew) {
                displayDriverRestartNote = "Windows shut down for the display driver but didn’t start again. \(reason)"
            }
        }
    }
}

struct HvfDisplayDriverNoticeView: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        if let notice = session.displayDriverNotice {
            HvfNoticeStrip(identifier: "bridgevm.display.display-driver", dismiss: {
                if notice == .unbound { session.displayDriverUnboundDismissed = true }
                else { session.displayDriverNoticeDismissed = true }
            }) {
                Text(notice.text)
                if notice == .restart, let note = session.displayDriverRestartNote {
                    Text(note).foregroundStyle(.orange)
                }
                if notice.offersInstall {
                    HvfDisplayDriverInstallButton(session: session, onInstalled: { _ in
                        HvfDisplayDriverFlag.record(session)
                    }, compact: true)
                } else if notice == .restart {
                    Button("Restart Now") { session.restartForDisplayDriver() }
                        .disabled(session.displayDriverRestarting || !session.terminationStopAvailable)
                        .accessibilityIdentifier("bridgevm.display.display-driver.restart")
                }
            }
        }
    }
}
