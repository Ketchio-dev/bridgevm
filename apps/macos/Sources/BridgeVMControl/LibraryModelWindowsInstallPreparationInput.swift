import Foundation

extension LibraryModel {
    func currentWindowsInstallPreparationInput(slug: String, repoRoot: URL) -> HvfWindowsInstallPlanInput? {
        guard let config = vms.first(where: { $0.slug == slug }),
              config.engineKind == .hvfEngine, config.installPending == true,
              let request = HvfWindowsInstallRequest.load(bundlePath: config.bundlePath) else { return nil }
        return HvfWindowsInstallPlanInput(config: config, request: request,
                                         libraryRoot: rootURL, repoRoot: repoRoot)
    }

    func windowsInstallPreparationFailure(slug: String) -> String {
        guard let config = vms.first(where: { $0.slug == slug }),
              config.engineKind == .hvfEngine, config.installPending == true else {
            return HvfWindowsInstallPreparation.missingMessage
        }
        return "이 VM의 저장된 Windows 설치 정보를 읽을 수 없습니다. 파일과 저장 장치를 확인한 뒤 다시 확인하세요."
    }
}
