import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif

struct CreateVMSheet: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.dismiss) var dismiss
    @State var mode: Mode = .windowsHVFInstall
    @State var name = "Windows 11"
    @State var isoPath: String = ""
    @State var drivers = CreateVMDriverSelection()
    @State var hvfTargetPath: String = ""
    @State var hvfVarsPath: String = ""
    @State var diskGiB = 64
    @State var hvfNetwork = true
    @State var storageDir: URL? = nil
    @State var resIndex = 1
    @State private var showAdvanced = false
    @State var ramMiB = CreateVMMemoryOptions.current.recommendedMiB
    @State var cpuCount = 4
    var working: Bool { library.preparingNewVM }
    @State var error = ""
    @State var creationFailureCode = ""
    @State var creationState = CreateVMCreationState()
    /// The disk container being converted, and its percent.
    @State var conversion: (format: HvfImportedDiskFormat, percent: Int)?
    let resolutions = [(1280, 800), (1440, 900), (1920, 1080), (2560, 1440)]
    enum Mode: Equatable { case windowsHVF, windowsHVFInstall }
    private var maxCPU: Int { max(1, library.hostCPU - 1) }
    /// Modes that allocate a brand-new blank disk the user can size.
    private var createsFreshDisk: Bool { mode == .windowsHVFInstall }

    private let diskOptions = [64, 96, 128, 256, 512]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Create a Windows VM").font(.title2.bold()).accessibilityAddTraits(.isHeader).padding(20)
            Divider()
            ScrollView { fields.padding(20).disabled(working || !creationState.permitsCreation) }
            Divider()
            CreateVMResourceSummary(cpuCount: cpuCount, ramMiB: ramMiB,
                                    diskGiB: createsFreshDisk ? diskGiB : nil)
                .padding(.horizontal, 20).padding(.top, 12)
            VMPreparationQuitStatus(library: library).padding(.horizontal, 20)
            creationFooter
        }
        .frame(width: 480, height: 640)
        .interactiveDismissDisabled(working)
    }

    private var fields: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 8) {
                CreateVMMethodChoiceTile(title: "Install from ISO", selected: mode == .windowsHVFInstall) {
                    mode = .windowsHVFInstall; autofillWin11()
                }
                .accessibilityIdentifier("bridgevm.create.windows.install")
                CreateVMMethodChoiceTile(title: "Import an installed disk", selected: mode == .windowsHVF) {
                    mode = .windowsHVF
                }
            }

            HStack {
                Text("Name (required)").fixedSize()
                TextField("e.g. Windows 11", text: $name)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel("Virtual machine name (required)")
                    .accessibilityIdentifier("bridgevm.create.name")
            }
            if !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, VMLibrary.normalizedVMName(name) == nil {
                Text("Use 1–\(VMLibrary.maximumVMNameCharacters) characters without control characters. If the name is long, shorten it to fit the file name limit.")
                    .font(.caption)
                    .foregroundColor(.red)
            }

            if mode == .windowsHVF {
                Text("Choose a RAW, QCOW2, fixed-size VHD or VHDX disk with Windows Arm installed. BridgeVM creates a RAW copy in your library. If you omit the UEFI variables file, BridgeVM creates default boot settings.")
                    .font(.callout).foregroundColor(.secondary)
                HStack {
                    Button("Choose Disk…") { pickHVFTarget() }
                    Text(hvfTargetPath.isEmpty ? "No disk selected" : (hvfTargetPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                HStack {
                    Button("Choose UEFI Variables… (optional)") { pickHVFVars() }
                    Text(hvfVarsPath.isEmpty ? "No file selected" : (hvfVarsPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                Text(HvfWindowsImportNotice.varsHelp)
                    .font(.caption).foregroundColor(.secondary)
                Text(HvfWindowsImportNotice.bitLockerHelp)
                    .font(.caption).foregroundColor(.secondary)
                Text("The imported VM uses the standard display with 3D acceleration off.")
                    .font(.caption).foregroundColor(.secondary)
            } else {
                CreateWindowsInstallFields(isoPath: isoPath, drivers: drivers, pickISO: pickISO,
                    pickGuestPayload: pickGuestPayload, pickGuestPayloadManifest: pickGuestPayloadManifest,
                    fetchDrivers: fetchGuestDrivers)
            }

            DisclosureGroup("Advanced settings", isExpanded: $showAdvanced) {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text("Location").frame(width: 64, alignment: .leading)
                        Button("Choose Folder…") { pickStorage() }
                        Text(storageDir?.path ?? "VM library (default)")
                            .font(.caption).foregroundColor(.secondary).lineLimit(1).truncationMode(.middle)
                        if storageDir != nil {
                            Button { storageDir = nil } label: { Image(systemName: "xmark.circle.fill") }
                                .buttonStyle(.plain)
                                .accessibilityLabel("Reset storage location to the VM library")
                        }
                    }

                    CreateVMResolutionRow(usesGuestScreen: true,
                                          selection: $resIndex, resolutions: resolutions)

                    HStack {
                        Text("Memory").frame(width: 64, alignment: .leading)
                        Picker("Memory", selection: $ramMiB) {
                            ForEach(CreateVMMemoryOptions.current.choicesMiB, id: \.self) { Text("\($0 / 1024) GiB").tag($0) }
                        }.labelsHidden().frame(width: 150)
                        Spacer()
                    }
                    HStack {
                        Text("CPU").frame(width: 64, alignment: .leading)
                        Stepper(cpuCount == 1 ? "1 CPU core" : "\(cpuCount) CPU cores", value: $cpuCount, in: 1...maxCPU)
                            .frame(width: 150)
                        Spacer()
                    }
                    if createsFreshDisk {
                        HStack {
                            Text("Disk").frame(width: 64, alignment: .leading)
                            Picker("Disk", selection: $diskGiB) {
                                ForEach(diskOptions, id: \.self) { Text("\($0) GiB").tag($0) }
                            }.labelsHidden().frame(width: 150)
                            Spacer()
                        }
                    }
                    Toggle("Shared network (NAT)", isOn: $hvfNetwork).font(.callout)
                    Text("This Mac has \(String(format: "%.0f", library.hostMemGiB)) GiB of memory and \(library.hostCPU) CPU cores.")
                        .font(.caption).foregroundColor(.secondary)
                }
                .padding(.top, 8)
            }
            .font(.callout)
        }
    }

    private func pickISO() {
        FileSelection.choose(directories: false, extensions: ["iso", "img", "dmg"]) { isoPath = $0.path }
    }

    private func pickStorage() {
        FileSelection.choose(directories: true) { storageDir = $0 }
    }

    private func pickGuestPayload() {
        FileSelection.choose(directories: true) { drivers.choosePayload($0.path) }
    }

    private func pickGuestPayloadManifest() {
        FileSelection.choose(directories: false, extensions: ["tsv"]) { drivers.chooseManifest($0.path) }
    }

    /// Only on the user's click: the sheet opens with every source selection empty.
    private func fetchGuestDrivers() {
        guard drivers.beginFetch() else { return }
        Task.detached {
            let result = Result { try HvfWindowsDriverPayloadFetch.prepare() }
            await MainActor.run { drivers.finishFetch(result) }
        }
    }

    private func pickHVFTarget() {
        FileSelection.choose(directories: false, extensions: ["raw", "img", "qcow2", "vhd", "vhdx"]) { hvfTargetPath = $0.path }
    }

    private func pickHVFVars() {
        FileSelection.choose(directories: false, extensions: ["fd", "vars"]) { hvfVarsPath = $0.path }
    }

    private var mediaReady: Bool {
        switch mode {
        case .windowsHVF:
            return !hvfTargetPath.isEmpty
        case .windowsHVFInstall:
            return !isoPath.isEmpty && drivers.ready
        }
    }

    /// What Create still needs, in the order the sheet asks for it. Create was greyed out with no reason
    /// until drivers were chosen.
    var footerHint: String? {
        guard !working, creationState.permitsCreation, error.isEmpty else { return nil }
        switch mode {
        case .windowsHVF where hvfTargetPath.isEmpty:
            return "Choose the Windows disk to continue."
        case .windowsHVFInstall where isoPath.isEmpty:
            return "Choose the Windows 11 for Arm ISO to continue."
        case .windowsHVFInstall where drivers.status == .preparing:
            return nil // the drivers section shows its progress
        case .windowsHVFInstall where !drivers.ready:
            return "Choose Download Drivers, or your driver folder and its manifest, to continue."
        default:
            return name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? CreateVMCreationFooter.nameHint : nil
        }
    }

    var canCreate: Bool {
        !working && creationState.permitsCreation && mediaReady && VMLibrary.normalizedVMName(name) != nil
    }

    private func autofillWin11() {
        guard isoPath.isEmpty else { return }
        // Downloads only: a second entry named one developer's folder under
        // /Users/user, which on another machine is either meaningless or a file
        // the user never chose.
        let candidate = "\(NSHomeDirectory())/Downloads/Win11_25H2_English_Arm64_v2.iso"
        if FileManager.default.fileExists(atPath: candidate) { isoPath = candidate }
    }

}

extension CreateVMSheet {
    var creationFooter: some View {
        CreateVMCreationFooter(working: working, error: error, failureCode: creationFailureCode,
            canCreate: canCreate, hint: footerHint,
            cancel: { dismiss() }, create: create,
            recoveryAvailable: !creationState.permitsCreation, recover: recoverCreatedVM,
            workingLabel: conversion.map { $0.format.conversionLabel(percent: $0.percent) } ?? "")
    }
}

extension CreateVMSheet {
    func recoverCreatedVM() {
        guard !working, !creationState.permitsCreation else { return }
        let loaded = creationState.recover(adopt: library.add)
        if loaded { dismiss() }
        else { error = creationState.recoveryFailureMessage }
    }
}

extension CreateVMSheet {
    func completeCreation(_ outcome: HVFCreationOutcome?) {
        conversion = nil
        let result = creationState.complete(outcome, adopt: library.add)
        creationFailureCode = result.code
        error = result.message
        if result.dismiss { dismiss() }
        library.preparingNewVM = false
        library.finishVMPreparation(succeeded: result.dismiss)
    }
}
