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

    /// Latch once per run: later resize replies neither reopen a dismissal nor clear the evidence.
    func displayDriverReported(_ output: String) {
        guard config.libraryContext?.config.virtioDisplayDriverInstalled == true else { return }
        if output.split(whereSeparator: \.isNewline).contains(where: {
            HvfDisplayDriverNotice.virtio(in: String($0)) == .some(.none)
        }) { displayDriverUnbound = true }
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
                if notice == .unbound { session.displayDriverUnboundDismissed = true }
                else { session.displayDriverNoticeDismissed = true }
            }) {
                Text(notice == .unbound
                     ? "디스플레이 드라이버가 Windows에 연결되지 않아 화면을 창 크기에 맞출 수 없으니 Windows 장치 관리자에서 디스플레이 어댑터의 드라이버와 오류 상태를 확인하세요."
                     : notice == .install ? "창 크기에 맞춰 Windows 화면을 바꾸려면 디스플레이 드라이버가 필요합니다"
                     : "다시 시작하면 적용됩니다")
                if notice == .install {
                    HvfDisplayDriverInstallButton(session: session, onInstalled: { _ in
                        HvfDisplayDriverFlag.record(session)
                    }, compact: true)
                } else if notice == .restart {
                    Button("지금 다시 시작") { session.restartForDisplayDriver() }
                        .disabled(session.displayDriverRestarting || !session.terminationStopAvailable)
                        .accessibilityIdentifier("bridgevm.display.display-driver.restart")
                }
            }
        }
    }
}
