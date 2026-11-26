import Foundation

/// Presentation never exposes error type names, paths or implementation details.
/// Keep both the reflected error and Foundation's description in diagnostics.
enum WindowsFileFailureFeedback {
    static let preparation = "BridgeVM couldn’t prepare the Windows installation files. Check that the VM folder is writable and the drive has free space. If the problem persists, reinstall BridgeVM and retry."
    static let source = "BridgeVM couldn’t use the Windows installation files. Wait for other installations to finish, check access to the VM folder, then retry."
    static let finalization = "BridgeVM couldn’t save the Windows installation. Keep the VM files, check that the drive is connected and has free space, then retry."
    static let recovery = "BridgeVM couldn’t recover the interrupted Windows installation. Keep the VM files, check that the drive is connected and writable, then retry."
    static let importing = "Windows 파일을 가져오지 못했으니 디스크와 부팅 설정 파일의 접근 권한과 저장 장치의 여유 공간을 확인하고 다시 시도하세요."
    static let publication = "VM 등록 정보를 저장하지 못했으니 라이브러리 폴더의 접근 권한과 여유 공간을 확인하고 다시 시도하세요."
    static let unsynced = "VM 파일은 보존했지만 저장 완료를 확인하지 못했으니 라이브러리에서 VM 등록 상태를 확인하세요."
    private static let importingOutOfSpace = "대상 볼륨의 공간이 부족해 디스크를 가져오지 못했습니다. 공간을 확보한 뒤 다시 시도하세요."

    static func diagnostic(_ stage: String, _ error: Error) -> String {
        "Windows file operation failed [\(stage)]: \(String(reflecting: error)); \(error.localizedDescription)"
    }

    static func isImportOutOfSpace(_ error: Error) -> Bool {
        let ns = error as NSError
        switch ns.domain {
        case NSCocoaErrorDomain:
            return ns.code == CocoaError.Code.fileWriteOutOfSpace.rawValue
        case "BridgeVM.MediaImport":
            // The converter and the snapshot helper each emit a fixed sentence for ENOSPC.
            let detail = ns.localizedDescription
            return detail == importingOutOfSpace
                || detail == "No space left on device. Free space on the destination volume and try again."
        default:
            return false
        }
    }

    static func importRemedy(for error: Error) -> String? {
        if let error = error as? CocoaError, error.code == .executableNotLoadable {
            return "디스크 가져오기 도구를 실행할 수 없으니 BridgeVM 앱을 다시 설치하거나 소스에서 만든 앱이라면 패키지를 다시 빌드하세요."
        }
        return isImportOutOfSpace(error) ? importingOutOfSpace : nil
    }

    static func reportImport(_ stage: String, _ error: Error) -> String {
        report(stage, error, sentence: importRemedy(for: error) ?? importing)
    }

    static func report(_ stage: String, _ error: Error, sentence: String) -> String {
        NSLog("%@", diagnostic(stage, error))
        let ns = error as NSError
        guard ns.domain == "BridgeVM.MediaImport" else { return sentence }
        let detail = ns.localizedDescription
        switch detail {
        // These converter sentences contain fixed guidance. Match the entire sentence
        // before using it in the app, since other helper output can contain file paths.
        case importingOutOfSpace,
             "동적 VHD는 Hyper-V에서 고정 크기 VHD 또는 VHDX로 변환한 뒤 다시 가져오세요.",
             "이 QCOW2 압축 형식을 지원하지 않으니 원래 VM 앱에서 RAW로 변환한 뒤 다시 가져오세요.",
             "이 VHDX는 4K 논리 섹터 디스크라 BridgeVM에서 부팅할 수 없으니 512바이트 논리 섹터 디스크에 설치한 Windows를 가져오세요.",
             "이 디스크는 다른 디스크를 기반으로 하는 차이 디스크이니 Hyper-V에서 부모 디스크와 병합한 뒤 다시 가져오세요.",
             "이 VHDX에는 반영되지 않은 변경 기록이 남아 있으니 Windows에서 이 VHDX를 한 번 연결했다가 분리한 뒤 다시 시도하세요.":
            return detail
        case _ where detail.hasPrefix("이 디스크는 다른 디스크를 기반으로"):
            return "이 디스크는 다른 디스크를 기반으로 하니 원래 VM 앱에서 하나의 디스크로 합친 뒤 다시 가져오세요."
        case _ where detail.hasPrefix("이 디스크는 지원하지 않는 형식 기능"):
            return "지원하지 않는 디스크 형식이므로 원래 VM 앱에서 디스크를 변환한 뒤 다시 가져오세요."
        case _ where detail.hasPrefix("디스크 이미지가 손상되었거나 구조가 올바르지"):
            return "디스크 이미지가 손상되었거나 구조가 올바르지 않으니 원래 VM 앱에서 디스크를 점검한 뒤 다시 시도하세요."
        default:
            return sentence
        }
    }
}
