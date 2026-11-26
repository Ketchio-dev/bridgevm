import Foundation

/// Presentation never exposes error type names, paths or implementation details.
/// Keep both the reflected error and Foundation's description in diagnostics.
enum WindowsFileFailureFeedback {
    static let preparation = "Windows 설치 파일을 준비하지 못했으니 BridgeVM을 다시 설치하고 VM 폴더의 접근 권한과 여유 공간을 확인한 뒤 다시 시도하세요."
    static let source = "Windows 설치 파일을 사용할 수 없으니 다른 설치 작업이 끝났는지와 VM 폴더의 접근 권한을 확인하고 다시 시도하세요."
    static let finalization = "Windows 설치 결과를 저장하지 못했으니 VM 파일을 지우지 말고 저장 장치의 연결과 여유 공간을 확인한 뒤 다시 시도하세요."
    static let recovery = "중단된 Windows 설치를 복구하지 못했으니 VM 파일을 보존하고 저장 장치의 연결과 접근 권한을 확인한 뒤 다시 시도하세요."
    static let importing = "Could not import the Windows files. Check access to the disk image and firmware files and make sure the destination has enough free space."
    static let publication = "Could not save the VM registration. Check that the library folder is writable and has enough free space."
    static let unsynced = "BridgeVM kept the VM files but could not confirm the save. Check the VM in your library."
    private static let importingOutOfSpace = "No space left on device. Free space on the destination volume and try again."

    static func diagnostic(_ stage: String, _ error: Error) -> String {
        "Windows file operation failed [\(stage)]: \(String(reflecting: error)); \(error.localizedDescription)"
    }

    static func isImportOutOfSpace(_ error: Error) -> Bool {
        let ns = error as NSError
        switch ns.domain {
        case NSCocoaErrorDomain:
            return ns.code == CocoaError.Code.fileWriteOutOfSpace.rawValue
        case "BridgeVM.MediaImport":
            // Recognize the current helper and older converters used by source builds.
            let detail = ns.localizedDescription
            return detail == importingOutOfSpace
                || detail == "대상 볼륨의 공간이 부족해 디스크를 가져오지 못했습니다. 공간을 확보한 뒤 다시 시도하세요."
        default:
            return false
        }
    }

    static func importRemedy(for error: Error) -> String? {
        if let error = error as? CocoaError, error.code == .executableNotLoadable {
            return "The disk import tool could not start. Reinstall BridgeVM or rebuild the app if you built it from source."
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
             "Choose a QCOW2, fixed-size VHD, or VHDX disk to convert.",
             "This QCOW2 uses unsupported compression. Export it as RAW from the original VM app and import the copy.",
             "This QCOW2 disk is encrypted. Use qemu-img to export a decrypted copy and import it.",
             "This disk depends on a backing file. Export a standalone disk from the original VM app and import the copy.",
             "This QCOW2 stores data in a separate file. Use qemu-img to export a standalone disk and import the copy.",
             "This QCOW2 uses extended L2 tables. Use qemu-img to convert it to RAW and import the copy.",
             "This QCOW2 is still in use or was closed unexpectedly. Shut down the source VM cleanly and try again.",
             "This QCOW2 is marked as corrupt. Check a copy with qemu-img check and import it after repair.",
             "This VHDX has pending changes. Attach it in Windows, detach it cleanly, then try again.",
             "Convert the dynamic VHD to a fixed-size VHD or VHDX in Hyper-V, then import the converted disk.",
             "This differencing disk depends on a parent disk. Merge the chain in Hyper-V, then import the merged disk.",
             "This VHDX uses 4 KB logical sectors. Import Windows from a disk that uses 512-byte logical sectors.",
             "Choose a disk with a virtual size of 16 TiB or less.":
            return detail
        // Source builds can pick up a converter packaged before the English UI.
        case "동적 VHD는 Hyper-V에서 고정 크기 VHD 또는 VHDX로 변환한 뒤 다시 가져오세요.":
            return "Convert the dynamic VHD to a fixed-size VHD or VHDX in Hyper-V, then import the converted disk."
        case "이 QCOW2 압축 형식을 지원하지 않으니 원래 VM 앱에서 RAW로 변환한 뒤 다시 가져오세요.":
            return "This QCOW2 uses unsupported compression. Export it as RAW from the original VM app and import the copy."
        case "이 VHDX는 4K 논리 섹터 디스크라 BridgeVM에서 부팅할 수 없으니 512바이트 논리 섹터 디스크에 설치한 Windows를 가져오세요.":
            return "This VHDX uses 4 KB logical sectors. Import Windows from a disk that uses 512-byte logical sectors."
        case "이 디스크는 다른 디스크를 기반으로 하는 차이 디스크이니 Hyper-V에서 부모 디스크와 병합한 뒤 다시 가져오세요.":
            return "This differencing disk depends on a parent disk. Merge the chain in Hyper-V, then import the merged disk."
        case "이 VHDX에는 반영되지 않은 변경 기록이 남아 있으니 Windows에서 이 VHDX를 한 번 연결했다가 분리한 뒤 다시 시도하세요.":
            return "This VHDX has pending changes. Attach it in Windows, detach it cleanly, then try again."
        case "이 디스크는 다른 디스크를 기반으로 하니 qemu-img로 하나의 디스크로 합친 뒤 다시 가져오세요.":
            return "This disk depends on a backing file. Export a standalone disk from the original VM app and import the copy."
        case _ where detail.hasPrefix("This disk uses an unsupported feature (") || detail.hasPrefix("이 디스크는 지원하지 않는 형식 기능"):
            return "This disk uses an unsupported feature. Export it as RAW from the original VM app and import the copy."
        case _ where detail.hasPrefix("The disk image is damaged (") || detail.hasPrefix("디스크 이미지가 손상되었거나 구조가 올바르지"):
            return "The disk image is damaged. Check it in the original VM app and try again."
        default:
            return sentence
        }
    }
}
