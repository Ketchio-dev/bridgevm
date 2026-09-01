import Foundation

enum BridgeVMControlAppLaunch {
    @MainActor static func libraryModel() -> LibraryModel {
        let arguments = Array(CommandLine.arguments.dropFirst())
        let options = try? BridgeVMControlLaunchOptions.parse(arguments: arguments)
        return LibraryModel(
            rootURL: options?.e2eLibraryRoot ?? VMLibrary.root,
            e2eUnattendedPath: options?.e2eUnattendedPath?.path)
    }
}
