import SwiftUI

extension CreateVMSheet {
    func create() {
        let selectedTemplate = template
        if WindowsHVFProductPolicy.requiresTemplate(mode) && selectedTemplate == nil {
            error = "템플릿 VM이 없습니다"
            return
        }
        working = true
        error = ""
        guard let normalizedName = VMLibrary.normalizedVMName(name) else {
            error = "VM 이름은 제어문자 없이 1~\(VMLibrary.maximumVMNameCharacters)자이며 파일 ID 제한 안이어야 합니다."
            working = false
            return
        }
        let selectedMode = mode
        let selectedISO = isoPath
        let selectedPayload = guestPayloadPath
        let selectedManifest = guestPayloadManifestPath
        let e2eUnattendedPath = library.e2eUnattendedPath
        let target = hvfTargetPath
        let vars = hvfVarsPath
        if selectedMode == .windowsHVF,
           let importError = VMLibrary.windowsHVFImportError(
            targetDiskPath: target, varsPath: vars) {
            error = importError
            working = false
            return
        }
        let storage = storageDir
        let width = resolutions[resIndex].0
        let height = resolutions[resIndex].1
        let disk = diskGiB
        let memory = ramMiB
        let cpu = cpuCount
        let network = hvfNetwork
        let libraryRoot = library.rootURL
        Task.detached {
            let config: VMConfig?
            switch selectedMode {
            case .ubuntu:
                config = selectedTemplate.flatMap { VMLibrary.cloneUbuntu(
                    name: normalizedName, template: $0, storageDir: storage,
                    width: width, height: height, memMiB: memory, cpuCount: cpu) }
            case .iso:
                config = selectedTemplate.flatMap { VMLibrary.createFromISO(
                    name: normalizedName, isoPath: selectedISO, template: $0,
                    storageDir: storage, width: width, height: height,
                    diskGiB: disk, memMiB: memory, cpuCount: cpu) }
            case .windows:
                config = selectedTemplate.flatMap { VMLibrary.createWindows(
                    name: normalizedName, isoPath: selectedISO, template: $0,
                    storageDir: storage, width: width, height: height,
                    diskGiB: disk, memMiB: memory, cpuCount: cpu) }
            case .windowsHVF:
                config = VMLibrary.createWindowsHVF(
                    name: normalizedName, targetDiskPath: target, varsPath: vars,
                    storageDir: storage, width: width, height: height,
                    memMiB: memory, cpuCount: cpu, networkEnabled: network,
                    injectViogpu3d: false, libraryRoot: libraryRoot)
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
            await MainActor.run {
                working = false
                if let config, library.add(config) {
                    dismiss()
                } else {
                    error = "생성 또는 VM 라이브러리 저장 실패"
                }
            }
        }
    }
}
