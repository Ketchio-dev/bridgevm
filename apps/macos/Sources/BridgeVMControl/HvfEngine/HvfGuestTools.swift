import Foundation
import SwiftUI

enum HvfGuestTools {
    static let missing = "게스트 도구가 설치되지 않아 클립보드와 공유 폴더를 사용할 수 없으니 ‘게스트 도구 설치’를 누르세요."
    static let nextStart = "Windows를 종료했다가 다시 시작한 뒤 새로 보이는 드라이브에서 install.cmd를 실행하세요."
    static let attached = "Windows의 새 드라이브에서 install.cmd를 실행하고 관리자 권한을 허용하세요."
    static let failed = "게스트 도구 드라이브를 준비하지 못했으니 Mac의 저장 공간을 확인하고 다시 누르세요."

    static func image(_ config: HvfEngineConfig) -> URL? {
        config.libraryContext.map { URL(fileURLWithPath: $0.config.bundlePath).appendingPathComponent("metadata/guest-tools.raw") }
    }

    static func eligible(_ config: HvfEngineConfig) -> Bool {
        guard let vm = config.libraryContext?.config, vm.installPending != true else { return false }
        return ![HvfWindowsInstallRequest.fileName, HvfWindowsInstallRequest.doneFileName].contains {
            FileManager.default.fileExists(atPath: vm.bundlePath + "/" + $0)
        }
    }

    static func attachment(_ config: HvfEngineConfig) -> String? {
        guard let image = image(config), FileManager.default.fileExists(atPath: image.path),
              !FileManager.default.fileExists(atPath: config.evidenceDir + "/guest-agent-seen") else { return nil }
        return image.path
    }

    static func build(repoRoot: URL, image: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [repoRoot.appendingPathComponent("scripts/build-guest-tools-volume.sh").path,
            repoRoot.appendingPathComponent("scripts/win-assets").path, image.path]
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw CocoaError(.fileWriteUnknown) }
    }
}

extension HvfEngineSession {
    func prepareGuestTools() {
        guard !guestToolsPreparing, let image = HvfGuestTools.image(config) else { return }
        guestToolsPreparing = true
        let root = repoRoot
        Task {
            do {
                if !FileManager.default.fileExists(atPath: image.path) {
                    try await Task.detached { try HvfGuestTools.build(repoRoot: root, image: image) }.value
                }
                guestToolsMessage = connectionState == .stopped ? HvfGuestTools.attached : HvfGuestTools.nextStart
            } catch { guestToolsMessage = HvfGuestTools.failed }
            guestToolsPreparing = false
        }
    }
}

struct HvfGuestToolsStrip: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var notice: HvfAgentSilenceNotice
    @State private var dismissed = false

    init(session: HvfEngineSession) {
        self.session = session
        self.notice = session.agentSilenceNotice
    }

    var body: some View {
        if !dismissed, !session.serviceStarted,
           notice.shown == .missingTools || session.guestToolsMessage != nil {
            HvfNoticeStrip(identifier: "bridgevm.display.guest-tools", dismiss: { dismissed = true }) {
                HvfGuestToolsAction(session: session)
            }
        }
    }
}

struct HvfGuestToolsAction: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var notice: HvfAgentSilenceNotice

    init(session: HvfEngineSession) {
        self.session = session
        self.notice = session.agentSilenceNotice
    }

    var body: some View {
        if !session.serviceStarted, notice.shown == .missingTools || session.guestToolsMessage != nil {
            VStack(alignment: .leading, spacing: 6) {
                if let text = session.guestToolsMessage { Text(text).font(.callout) }
                Button("게스트 도구 설치", action: session.prepareGuestTools)
                    .disabled(session.guestToolsPreparing)
                    .accessibilityIdentifier("bridgevm.runtime.guest-tools.install")
            }
        }
    }
}
