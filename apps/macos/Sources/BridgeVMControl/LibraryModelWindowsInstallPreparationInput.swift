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
        return "Couldn’t read this VM’s installation settings. Check its files and storage, then choose Check Again."
    }
}
