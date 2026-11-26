import Foundation

enum HvfWindowsInstallStage: Equatable {
    case idle
    case validating
    case preparingSource
    case installing
    case finalizing
    case recovering
    case cancelling
    case cancelled
    case done
    case failed(String)

    var isRunning: Bool {
        switch self {
        case .validating, .preparingSource, .installing, .finalizing, .recovering, .cancelling: return true
        default: return false
        }
    }
}

extension HvfWindowsInstallStage {
    var label: String {
        switch self {
        case .idle: return "Ready to install"
        case .validating: return "Checking installation files"
        case .preparingSource: return "Preparing installation media"
        case .installing: return "Installing Windows"
        case .finalizing: return "Saving the VM"
        case .recovering: return "Recovering the installation"
        case .cancelling: return "Cancelling…"
        case .cancelled: return "Cancelled"
        case .done: return "Complete"
        case .failed: return "Failed"
        }
    }

    var primaryActionTitle: String {
        switch self {
        case .failed, .cancelled: return "Try Again"
        case .done: return "Installation Complete"
        default: return "Start Installation"
        }
    }
}

extension HvfWindowsInstallStage {
    static let steps = ["Check files", "Prepare media", "Install Windows", "Save VM"]

    /// Builder log lines can include private paths. Only show the known operation.
    static func sourceActivity(for line: String) -> String? {
        if line == "[build-src] attaching ISO" { return "Mounting the installer…" }
        if line.hasPrefix("[build-src] copying ISO tree") { return "Copying setup files from the ISO…" }
        if line.hasPrefix("[build-src] splitting install.wim") { return "Splitting the Windows installation image…" }
        if line.hasPrefix("[build-src] injecting bvinstall payload") { return "Preparing Windows boot files…" }
        if line == "[build-src] verifying payload + boot files" { return "Verifying installation files…" }
        return nil
    }

    var installationStep: Int? {
        switch self {
        case .validating: 0
        case .preparingSource: 1
        case .installing: 2
        case .finalizing: 3
        case .idle, .recovering, .cancelling, .cancelled, .done, .failed: nil
        }
    }
}
