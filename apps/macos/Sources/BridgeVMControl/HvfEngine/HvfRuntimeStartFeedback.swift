extension HvfEngineSession {
    var runtimePendingWorkText: String? {
        if hasPendingRuntimeMutation { return "VM operation in progress…" }
        if let start = guiStartOperation, start.workerPending {
            return start.invalidationReason == nil ? "Preparing to start Windows…" : "Waiting for startup cleanup…"
        }
        return nil
    }

    var runtimeSupervisionFailureText: String? {
        // A failed lifecycle receipt alone says nothing about storage, and must not hide
        // a specific startup error or the runner-death sentence.
        if guiStartFailureText == nil, diskSyncNotice.finalSaveFailed {
            return "Windows를 종료하면서 디스크를 Mac에 안전하게 저장하지 못했으니 Mac의 저장 공간과 디스크 연결을 확인하세요."
        }
        guard !runnerDiedWithoutCleanup, ownedController?.supervisionLost == true else { return nil }
        return "BridgeVM lost contact with Windows, so choose Force Stop to stop the VM."
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
            case .keyAccessDenied:
                return "Unlock your login keychain and try starting Windows again."
            case .keyStateMissing:
                return "To recover the missing key for this VM’s security data, restore your login keychain from a backup."
            case .keyStateInvalid:
                return "BridgeVM could not read this VM’s security data, so restore the VM settings and login keychain from a backup."
            case .keyGeneration:
                return "BridgeVM could not create a security key for Windows, so restart your Mac and try again."
            case .preparation:
                return "BridgeVM could not prepare the VM files, so check access to the VM folder and the available disk space."
            case .helper:
                return "Reinstall BridgeVM to restore a missing or damaged app component."
            case .keyDelivery:
                return "BridgeVM could not pass this VM’s security data to its helper, so reinstall BridgeVM and try again."
            case let .readiness(codes):
                // Prefer the first user-adjustable blocker, preserving readiness order.
                for code in codes {
                    switch code {
                    case "target-disk-missing":
                        return "To recover the missing Windows disk, reconnect its drive or restore the disk file to its original location."
                    case "target-disk-access":
                        return "BridgeVM cannot access the Windows disk, so check its read and write permissions in Finder."
                    case "target-disk-empty":
                        return "The Windows disk file is empty, so restore the original disk from a backup to the same location."
                    case "uefi-vars-missing":
                        return "Look for the missing Windows boot settings file in the VM folder or restore it from a backup."
                    case "uefi-vars-access":
                        return "BridgeVM cannot access the Windows boot settings, so check the file’s read and write permissions in Finder."
                    case "uefi-vars-size":
                        return "The Windows boot settings file has the wrong size, so restore the 64 MiB file from a backup."
                    case "ram-range":
                        return "Set the VM’s memory between 1024 and 65536 MiB, then start Windows again."
                    case "cpu-range":
                        return "Choose 1 to 64 processors in VM settings, then start Windows again."
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
                    return "BridgeVM could not start Windows with these settings, so check the VM settings or restore them from a backup."
                default:
                    return "Windows could not start, so review the error below and try again.\n\(detail)"
                }
            case .keyAccess, .processLaunch:
                return "Windows could not start, so review the error below and try again.\n\(detail)"
            }
        case .ownedLaunchAccepted, .observedAttachment: return nil
        }
    }
}
