import Foundation

/// Presentation never exposes error type names, paths or implementation details.
/// Keep both the reflected error and Foundation's description in diagnostics.
enum WindowsFileFailureFeedback {
    static let preparation = "BridgeVM couldn’t prepare the Windows installation files. Check that the VM folder is writable and the drive has free space. If the problem persists, reinstall BridgeVM and retry."
    static let source = "BridgeVM couldn’t use the Windows installation files. Wait for other installations to finish, check access to the VM folder, then retry."
    static let finalization = "BridgeVM couldn’t save the Windows installation. Keep the VM files, check that the drive is connected and has free space, then retry."
    static let recovery = "BridgeVM couldn’t recover the interrupted Windows installation. Keep the VM files, check that the drive is connected and writable, then retry."
    static let importing = "Could not import the Windows files. Check access to the disk image and firmware files and make sure the destination has enough free space."
    static let publication = "Could not save the VM registration. Check that the library folder is writable and has enough free space."
    static let unsynced = "BridgeVM kept the VM files but could not confirm the save. Check your library for the saved VM before creating another."
    private static let importingOutOfSpace = "No space left on device. Free space on the destination volume and try again."
    private static let importingInUse = "Shut down the VM that uses this disk, then import again."

    static func diagnostic(_ stage: String, _ error: Error) -> String {
        "Windows file operation failed [\(stage)]: \(String(reflecting: error)); \(error.localizedDescription)"
    }

    static func isImportOutOfSpace(_ error: Error) -> Bool {
        let ns = error as NSError
        switch ns.domain {
        case NSCocoaErrorDomain:
            return ns.code == CocoaError.Code.fileWriteOutOfSpace.rawValue
        case "BridgeVM.MediaImport":
            return ns.localizedDescription == importingOutOfSpace
        default:
            return false
        }
    }

    static func importRemedy(for error: Error) -> String? {
        if let error = error as? CocoaError, error.code == .executableNotLoadable {
            return "The disk import tool could not start. Reinstall BridgeVM or rebuild the app if you built it from source."
        }
        if isImportOutOfSpace(error) { return importingOutOfSpace }
        // The helper's writer-lease refusal (snapshot_pair.rs writer_refusal), also matched by the snapshot UI.
        let ns = error as NSError
        if ns.domain == "BridgeVM.MediaImport", ns.localizedDescription.contains("media is in use by a running VM") {
            return importingInUse
        }
        return nil
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
             "This QCOW2 disk is encrypted. Export a decrypted disk with qemu-img and import the result.",
             "This disk depends on a backing file. Export a standalone disk from the original VM app and import the copy.",
             "This QCOW2 stores data in a separate file. Export a standalone disk with qemu-img and import the result.",
             "This QCOW2 uses extended L2 tables. Convert it to RAW with qemu-img and import the result.",
             "This QCOW2 is still in use or was closed unexpectedly. Shut down the source VM cleanly and try again.",
             "This QCOW2 is marked as corrupt. Run qemu-img check on a duplicate and import the repaired disk.",
             "This VHDX has pending changes. Attach it in Windows, detach it cleanly, then try again.",
             "Convert the dynamic VHD to a fixed-size VHD or VHDX in Hyper-V, then import the converted disk.",
             "This differencing disk depends on a parent disk. Merge the chain in Hyper-V, then import the merged disk.",
             "This VHDX uses 4 KB logical sectors. Import Windows from a disk that uses 512-byte logical sectors.",
             "Choose a disk with a virtual size of 16 TiB or less.":
            return detail
        case _ where detail.hasPrefix("This disk uses an unsupported feature ("):
            return "This disk uses an unsupported feature. Export it as RAW from the original VM app and import the copy."
        case _ where detail.hasPrefix("The disk image is damaged ("):
            return "The disk image is damaged. Check it in the original VM app and try again."
        default:
            return sentence
        }
    }
}
