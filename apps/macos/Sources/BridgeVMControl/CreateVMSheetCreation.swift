import SwiftUI

extension CreateVMSheet {
    func create() {
        guard !working, creationState.permitsCreation else { return }
        guard mode != .windowsHVFInstall || drivers.status != .preparing else { return }
        creationFailureCode = ""
        error = ""
        guard let normalizedName = VMLibrary.normalizedVMName(name) else {
            creationFailureCode = "invalid-vm-name"
            error = "VM 이름은 제어문자 없이 1~\(VMLibrary.maximumVMNameCharacters)자이며 파일 ID 제한 안이어야 합니다."
            return
        }
        let selectedMode = mode
        let selectedISO = isoPath
        let selectedPayload = drivers.payloadPath
        let selectedManifest = drivers.manifestPath
        let e2eUnattendedPath = library.e2eUnattendedPath
        let target = hvfTargetPath
        let vars = hvfVarsPath
        if selectedMode == .windowsHVF,
           let importError = VMLibrary.windowsHVFImportError(
            targetDiskPath: target, varsPath: vars) {
            creationFailureCode = "hvf-import-invalid"
            error = importError
            return
        }
        library.preparingNewVM = true
        let storage = storageDir
        conversion = selectedMode == .windowsHVF ? HvfImportedDiskFormat.detect(path: target).map { ($0, 0) } : nil
        let width = resolutions[resIndex].0
        let height = resolutions[resIndex].1
        let disk = diskGiB
        let memory = ramMiB
        let cpu = cpuCount
        let network = hvfNetwork
        let libraryRoot = library.rootURL
        Task.detached {
            let config: HVFCreationOutcome?
            switch selectedMode {
            case .windowsHVF:
                config = VMLibrary.createWindowsHVF(
                    name: normalizedName, targetDiskPath: target, varsPath: vars,
                    storageDir: storage, width: width, height: height,
                    memMiB: memory, cpuCount: cpu, networkEnabled: network,
                    injectViogpu3d: false, libraryRoot: libraryRoot, converting: { format, percent in
                        Task { @MainActor in
                            if percent > conversion?.percent ?? -1 { conversion = (format, percent) }
                        }
                    })
            case .windowsHVFInstall:
                config = VMLibrary.createWindowsHVFInstall(
                    name: normalizedName, isoPath: selectedISO, diskGiB: disk,
                    injectViogpu3d: false, driverPackageDir: nil,
                    guestPayloadDirectory: selectedPayload,
                    guestPayloadManifest: selectedManifest,
                    e2eUnattendedPath: e2eUnattendedPath,
                    storageDir: storage, width: width, height: height,
                    memMiB: memory, cpuCount: cpu, networkEnabled: network,
                    libraryRoot: libraryRoot)
            }
            await MainActor.run { completeCreation(config) }
        }
    }
}
