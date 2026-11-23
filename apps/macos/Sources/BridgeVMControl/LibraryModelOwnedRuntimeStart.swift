import Foundation

extension LibraryModel {
    /// `bridgevm app start`: the library's Start button on the VM's current saved configuration.
    func requestCLIStart(vmID: String) -> NativeRuntimeStartAdmission {
        // The CLI or another tool may have saved vm.json after this app last read it, or published a
        // new VM (#684): read the library once, as the manual refresh does, and start what is saved.
        let current = vms.first { $0.slug == vmID }
        if current == nil || (try? NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false))
            .flatMap({ try? NativeCLIStatusConfigReader.read(library: $0, id: vmID) }) != current { reload() }
        guard let saved = vms.first(where: { $0.slug == vmID }) else {
            return .refused("이 라이브러리에 '\(vmID)' VM이 없습니다. `bridgevm app list`로 ID를 확인하세요.")
        }
        guard saved.engineKind == .hvfEngine else {
            return .refused("이 VM은 BridgeVM 엔진을 쓰지 않아 CLI로 시작할 수 없습니다.")
        }
        guard saved.installPending != true else {
            return .refused("Windows 설치가 끝나지 않은 VM입니다. 설치를 마친 뒤 시작하세요.")
        }
        guard let launch = HvfEngineConfig.libraryVM(saved, rootURL: rootURL,
                  e2eHostDiagnosticStopAdmitted: e2eHostDiagnosticStopAdmitted),
              let session = hvfRuntimeDetailSession(for: saved) else {
            return .refused("저장된 VM 실행 구성을 읽지 못했습니다.")
        }
        switch session.requestGUIStart(configuration: launch, policy: .requireNew) {
        case let .accepted(ticket): return .accepted(ticket, session)
        case let .refused(reason): return .refused(reason)
        }
    }
}
