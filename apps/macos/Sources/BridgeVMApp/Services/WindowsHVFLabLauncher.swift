import Foundation
#if canImport(AppKit)
import AppKit
#endif

struct WindowsHVFLabInstallation: Equatable {
    static let relativeBundlePath = "Contents/Applications/BridgeVMControl.app"

    let bundleURL: URL
    let executableURL: URL

    static func resolve(
        hostBundleURL: URL = Bundle.main.bundleURL,
        fileManager: FileManager = .default
    ) throws -> WindowsHVFLabInstallation {
        let bundleURL = hostBundleURL.appendingPathComponent(relativeBundlePath, isDirectory: true)
        let executableURL = bundleURL.appendingPathComponent("Contents/MacOS/BridgeVMControl")
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: bundleURL.path, isDirectory: &isDirectory), isDirectory.boolValue else {
            throw WindowsHVFLabLaunchError.missingBundle(bundleURL.path)
        }
        guard fileManager.isExecutableFile(atPath: executableURL.path) else {
            throw WindowsHVFLabLaunchError.missingExecutable(executableURL.path)
        }
        return WindowsHVFLabInstallation(bundleURL: bundleURL, executableURL: executableURL)
    }
}

enum WindowsHVFLabLaunchError: LocalizedError, Equatable {
    case missingBundle(String)
    case missingExecutable(String)
    case launchRejected(String)

    var errorDescription: String? {
        switch self {
        case let .missingBundle(path):
            return "Windows HVF Lab is not bundled at \(path)."
        case let .missingExecutable(path):
            return "Windows HVF Lab executable is missing at \(path)."
        case let .launchRejected(path):
            return "macOS refused to open Windows HVF Lab at \(path)."
        }
    }
}

@MainActor
enum WindowsHVFLabLauncher {
    static func open(hostBundleURL: URL = Bundle.main.bundleURL) throws {
        let installation = try WindowsHVFLabInstallation.resolve(hostBundleURL: hostBundleURL)
        #if canImport(AppKit)
        guard NSWorkspace.shared.open(installation.bundleURL) else {
            throw WindowsHVFLabLaunchError.launchRejected(installation.bundleURL.path)
        }
        #else
        throw WindowsHVFLabLaunchError.launchRejected(installation.bundleURL.path)
        #endif
    }
}
