extension HvfEngineSession {
    var runtimePendingWorkText: String? {
        if hasPendingRuntimeMutation { return "VM operation in progress…" }
        if let start = guiStartOperation, start.workerPending {
            return start.invalidationReason == nil ? "Preparing to start Windows…" : "Waiting for startup cleanup…"
        }
        return nil
    }

    var runtimeSupervisionFailureText: String? {
        if driveDisconnected, hasActiveRuntimeWork { return HvfDriveDisconnect.text }
        // A failed lifecycle receipt alone says nothing about storage, and must not hide
        // a specific startup error or the runner-death sentence.
        if guiStartFailureText == nil, diskSyncNotice.finalSaveFailed {
            return "BridgeVM couldn’t finish saving the Windows disk at shutdown. Check your Mac’s free space and drive connection."
        }
        if guiStartFailureText == nil, let runEndedText { return runEndedText }
        guard !runnerDiedWithoutCleanup, ownedController?.supervisionLost == true else { return nil }
        return "BridgeVM lost contact with Windows, so choose Force Stop to stop the VM."
    }

    /// A run that ended by itself with a failure: the VM process crashed or was killed, a restart
    /// failed, or the disk was still in use. It used to read "Windows has shut down.", like a
    /// shutdown from Windows' own Start menu.
    private var runEndedText: String? {
        guard !hasPendingRuntimeStart, let controller = ownedController, controller.operation == nil,
              controller.teardownConfirmed else { return nil }
        switch controller.ledger.complete?.failureCode {
        case "mediaAdmissionFailed":
            return "Windows couldn’t start because another process is still using this VM’s disk. Wait a minute, then start it again."
        case "helperFailed", "resetFailed", "startupFailed":
            return "Windows stopped unexpectedly. You can start it again."
        default: return nil
        }
    }

    /// What the dashboard, the display window and the Advanced view say about a run that needs
    /// attention: a disk not saved at shutdown or lost contact, else a start that failed.
    var runtimeFailureText: String? {
        runtimeSupervisionFailureText ?? guiStartFailureText
            ?? (connectionState == .stopped ? displayDriverRestartNote : nil)
    }

    var guiStartFailureText: String? {
        if let text = guiStartOperation?.outcome?.failureText { return text }
        return runnerDiedWithoutCleanup ? "Windows stopped unexpectedly, so wait a moment before starting it again." : nil
    }
}

extension HvfRuntimeStartOutcome {
    /// The one sentence the app (and the CLI) shows for a start that did not launch.
    var failureText: String? {
        switch self {
        case let .refused(reason): return reason
        case let .failed(stage, detail):
            switch stage {
            case .keyAccessLocked:
                return "Unlock your login keychain and try starting Windows again."
            case .keyAccessDenied:
                return "BridgeVM needs permission to use this VM’s security key. Start Windows again, enter your Mac password and choose Always Allow."
            case .keyStateMissing:
                return "To recover the missing key for this VM’s security data, restore your login keychain from a backup."
            case .keyStateInvalid:
                return "BridgeVM couldn’t read this VM’s security data, so restore the VM settings and login keychain from a backup."
            case .keyGeneration:
                return "BridgeVM couldn’t create a security key for Windows, so restart your Mac and try again."
            case .preparation:
                return "BridgeVM couldn’t prepare the VM files, so check access to the VM folder and the available disk space."
            case .helper:
                return "Reinstall BridgeVM to restore a missing or damaged app component."
            case .keyDelivery:
                return "BridgeVM couldn’t pass this VM’s security data to the VM, so reinstall BridgeVM and try again."
            case let .readiness(codes):
                // Prefer the first user-adjustable blocker, preserving readiness order.
                for code in codes {
                    switch code {
                    case "target-disk-missing":
                        return "To recover the missing Windows disk, reconnect its drive or restore the disk file to its original location."
                    case "target-disk-access":
                        return "BridgeVM can’t access the Windows disk, so check its read and write permissions in Finder."
                    case "target-disk-empty":
                        return "The Windows disk file is empty, so restore the original disk from a backup to the same location."
                    case "uefi-vars-missing":
                        return "Look for the missing Windows boot settings file in the VM folder or restore it from a backup."
                    case "uefi-vars-access":
                        return "BridgeVM can’t access the Windows boot settings, so check the file’s read and write permissions in Finder."
                    case "uefi-vars-size":
                        return "The Windows boot settings file has the wrong size, so restore the 64 MiB file from a backup."
                    case "ram-range":
                        return "Set the VM’s memory between 1024 and 65536 MiB, then start Windows again."
                    case "cpu-range":
                        return "Choose 1 to 64 CPUs in VM settings, then start Windows again."
                    case "share-invalid":
                        return "Choose the shared folder again in VM settings, then start Windows again."
                    case "share-incomplete":
                        return "To complete the shared folder setup, choose the folder again in VM settings."
                    default: continue
                    }
                }
                switch codes.first {
                case "boot-wrapper-missing", "signed-runner-missing", "swtpm-missing", "swtpm-signature":
                    return "Reinstall BridgeVM to restore a missing or damaged app component."
                case "watchdog-range", "evidence-not-directory", "control-path-collision", "vtpm-path-collision", "vtpm-key-id", "vtpm-state-not-directory":
                    return "BridgeVM couldn’t start Windows with these settings, so check the VM settings or restore them from a backup."
                default:
                    // An unfinished move or install cleanup: Refresh Library reads the library again.
                    return "BridgeVM couldn’t start Windows yet. Press Command-K in the library window and choose Refresh Library, then try again."
                }
            // The keychain's status code stays in the run's event log (HvfGUIStartCompletion).
            case .keyAccess:
                return "BridgeVM couldn’t use this VM’s security key in your login keychain, so check that the keychain is unlocked and start Windows again."
            // Unexpected: the system's own error follows, for the app and the CLI alike.
            case .processLaunch:
                return "BridgeVM couldn’t start the VM process, so quit and reopen BridgeVM and try again.\n\(detail)"
            }
        case .ownedLaunchAccepted, .observedAttachment: return nil
        }
    }
}
