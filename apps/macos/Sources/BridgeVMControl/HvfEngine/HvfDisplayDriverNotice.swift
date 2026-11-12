import SwiftUI

/// Only the 2D launch needs this driver. Installation during a run takes effect at the next launch.
enum HvfDisplayDriverNotice: Equatable {
    case install, restart

    static func shown(agentReady: Bool, installed: Bool?, experimental3D: Bool,
                      dismissed: Bool, restartRequired: Bool) -> Self? {
        guard agentReady, installed != true, !experimental3D, !dismissed else { return nil }
        return restartRequired ? .restart : .install
    }
}

extension HvfEngineSession {
    var displayDriverNotice: HvfDisplayDriverNotice? {
        HvfDisplayDriverNotice.shown(agentReady: HvfDisplayDriverInstall.agentReady(self),
            installed: config.libraryContext?.config.virtioDisplayDriverInstalled,
            experimental3D: config.virtioGpu3d, dismissed: displayDriverNoticeDismissed,
            restartRequired: displayDriverRestartRequired)
    }

    /// A Windows restart cannot attach a device. Join the normal graceful Stop, wait for full
    /// cleanup and the saved driver flag, then use the normal new-runtime start path.
    @discardableResult
    func restartForDisplayDriver() -> Task<Void, Never>? {
        guard !displayDriverRestarting, terminationStopAvailable,
              let identity = ownedProcessIdentity else { return nil }
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
            guard config.libraryContext?.config.virtioDisplayDriverInstalled == true else { return }
            _ = requestGUIStart(configuration: config, policy: .requireNew)
        }
    }
}

struct HvfDisplayDriverNoticeView: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        if let notice = session.displayDriverNotice {
            HvfNoticeStrip(identifier: "bridgevm.display.display-driver", dismiss: {
                session.displayDriverNoticeDismissed = true
            }) {
                Text(notice == .install
                     ? "창 크기에 맞춰 Windows 화면을 바꾸려면 디스플레이 드라이버가 필요합니다"
                     : "다시 시작하면 적용됩니다")
                if notice == .install {
                    HvfDisplayDriverInstallButton(session: session, onInstalled: { _ in
                        HvfDisplayDriverFlag.record(session)
                    }, compact: true)
                } else {
                    Button("지금 다시 시작") { session.restartForDisplayDriver() }
                        .disabled(session.displayDriverRestarting || !session.terminationStopAvailable)
                        .accessibilityIdentifier("bridgevm.display.display-driver.restart")
                }
            }
        }
    }
}
