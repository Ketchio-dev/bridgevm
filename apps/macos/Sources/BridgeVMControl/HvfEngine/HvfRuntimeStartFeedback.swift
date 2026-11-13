extension HvfEngineSession {
    var runtimePendingWorkText: String? {
        if hasPendingRuntimeMutation { return "작업 중" }
        if let start = guiStartOperation, start.workerPending {
            return start.invalidationReason == nil ? "시작 준비 중" : "작업 정리 확인 중"
        }
        if let start = ownedStartOperation, start.workerPending {
            return start.observation.failure == nil ? "시작 준비 중" : "작업 정리 확인 중"
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
        return "실행 중인 Windows와 연결이 끊겼으니 강제 중단으로 Windows를 멈추세요."
    }

    var guiStartFailureText: String? {
        if !hasPendingOwnedStart, let outcome = guiStartOperation?.outcome {
            switch outcome {
            case let .refused(reason): return reason
            case let .failed(stage, detail):
                switch stage {
                case .keyAccessDenied:
                    return "로그인 키체인에 접근할 수 없어 Windows를 시작하지 못했으니 키체인 잠금을 해제하고 다시 시도하세요."
                case .keyStateMissing:
                    return "Windows 보안 정보의 암호화 키가 없어 시작하지 못했으니 백업에서 로그인 키체인을 복원하고 다시 시도하세요."
                case .keyStateInvalid:
                    return "Windows 보안 정보를 사용할 수 없어 시작하지 못했으니 VM 설정과 로그인 키체인을 백업에서 복원하고 다시 시도하세요."
                case .keyGeneration:
                    return "Windows 보안 키를 만들지 못했으니 Mac을 재시동하고 다시 시도하세요."
                case .preparation:
                    return "VM 작업 파일을 준비하지 못했으니 VM 폴더의 접근 권한과 저장 장치의 여유 공간을 확인하고 다시 시도하세요."
                case .helper:
                    return "앱 실행 파일이 없거나 손상되어 Windows를 시작하지 못했으니 BridgeVM을 다시 설치하세요."
                case .keyDelivery:
                    return "Windows 보안 정보를 실행 도우미에 전달하지 못했으니 BridgeVM을 다시 설치하고 다시 시도하세요."
                case let .readiness(codes):
                    // Prefer the first user-adjustable blocker, preserving readiness order.
                    for code in codes {
                        switch code {
                        case "target-disk-missing":
                            return "Windows 디스크를 찾을 수 없어 시작하지 못했으니 VM 설정에서 디스크를 다시 선택하세요."
                        case "target-disk-access":
                            return "Windows 디스크에 접근할 수 없어 시작하지 못했으니 Finder에서 디스크 파일의 읽기 및 쓰기 권한을 확인하세요."
                        case "target-disk-empty":
                            return "Windows 디스크가 비어 있어 시작하지 못했으니 VM 설정에서 Windows가 설치된 디스크를 선택하세요."
                        case "uefi-vars-missing":
                            return "부팅 설정 파일을 찾을 수 없어 Windows를 시작하지 못했으니 VM 폴더에서 파일을 확인하거나 백업에서 복원하세요."
                        case "uefi-vars-access":
                            return "부팅 설정 파일에 접근할 수 없어 Windows를 시작하지 못했으니 Finder에서 해당 파일의 읽기 및 쓰기 권한을 확인하세요."
                        case "uefi-vars-size":
                            return "부팅 설정 파일의 크기가 올바르지 않아 Windows를 시작하지 못했으니 백업에서 64 MiB 크기의 파일을 복원하세요."
                        case "ram-range":
                            return "메모리 설정이 범위를 벗어나 Windows를 시작하지 못했으니 메모리를 1024~65536 MiB로 설정하세요."
                        case "cpu-range":
                            return "CPU 개수 설정이 범위를 벗어나 Windows를 시작하지 못했으니 CPU를 1~64개로 설정하세요."
                        case "share-invalid":
                            return "공유 폴더 설정이 올바르지 않아 Windows를 시작하지 못했으니 VM 설정에서 공유 폴더를 다시 선택하세요."
                        case "share-incomplete":
                            return "공유 폴더 설정이 완전하지 않아 Windows를 시작하지 못했으니 VM 설정에서 공유 폴더를 다시 선택하세요."
                        default: continue
                        }
                    }
                    switch codes.first {
                    case "boot-wrapper-missing", "signed-runner-missing", "swtpm-missing", "swtpm-signature":
                        return "앱 실행 파일이 없거나 손상되어 Windows를 시작하지 못했으니 BridgeVM을 다시 설치하세요."
                    case "watchdog-range", "evidence-not-directory", "control-path-collision", "vtpm-path-collision", "vtpm-key-id", "vtpm-state-not-directory":
                        return "VM 설정이 올바르지 않아 Windows를 시작하지 못했으니 VM 설정을 확인하거나 백업에서 복원하세요."
                    default:
                        return "Windows를 시작하지 못했으니 아래 오류 내용을 확인하고 다시 시도하세요.\n\(detail)"
                    }
                case .keyAccess, .processLaunch:
                    return "Windows를 시작하지 못했으니 아래 오류 내용을 확인하고 다시 시도하세요.\n\(detail)"
                }
            case .ownedLaunchAccepted, .observedAttachment: break
            }
        }
        return runnerDiedWithoutCleanup ? "Windows가 예기치 않게 종료되었으니 잠시 후 다시 시작하세요." : nil
    }
}
