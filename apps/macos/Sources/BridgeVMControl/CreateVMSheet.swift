import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif

// MARK: - Create sheet (gallery)
struct CreateVMSheet: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.dismiss) var dismiss
    @State var mode: Mode = .windowsHVFInstall
    @State var name = ""
    @State var isoPath: String = ""
    @State var guestPayloadPath: String = ""
    @State var guestPayloadManifestPath: String = ""
    @State var fetchingDrivers = false
    @State var driverFetchError = ""
    @State var hvfTargetPath: String = ""
    @State var hvfVarsPath: String = ""
    @State var diskGiB = 64
    @State var hvfNetwork = true
    @State var storageDir: URL? = nil
    @State var resIndex = 1
    @State private var showAdvanced = false
    @State var ramMiB = 6144
    @State var cpuCount = 4
    @State var working = false
    @State var error = ""
    @State var creationFailureCode = ""
    @State var creationState = CreateVMCreationState()
    /// The QCOW2 or VHDX import being converted, and its percent.
    @State var conversion: (format: HvfImportedDiskFormat, percent: Int)?
    let resolutions = [(1280, 800), (1440, 900), (1920, 1080), (2560, 1440)]
    enum Mode: Equatable { case windowsHVF, windowsHVFInstall }
    /// Memory choices in MiB, capped just under host physical RAM.
    private var ramOptions: [Int] {
        let hostMiB = Int(library.hostMemGiB * 1024)
        let ceiling = max(4096, hostMiB - 4096) // leave headroom for macOS
        return [2048, 4096, 6144, 8192, 12288, 16384, 24576, 32768].filter { $0 <= ceiling }
    }
    private var maxCPU: Int { max(1, library.hostCPU - 1) }
    /// Modes that allocate a brand-new blank disk the user can size.
    private var createsFreshDisk: Bool { mode == .windowsHVFInstall }

    private let diskOptions = [64, 96, 128, 256, 512]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("새 VM 만들기").font(.title2.bold()).padding(20)
            Divider()
            ScrollView { fields.padding(20).disabled(working || !creationState.permitsCreation) }
            Divider()
            CreateVMResourceSummary(cpuCount: cpuCount, ramMiB: ramMiB,
                                    diskGiB: createsFreshDisk ? diskGiB : nil)
                .padding(.horizontal, 20).padding(.top, 12)
            creationFooter
        }
        .frame(width: 480, height: 640)
        .interactiveDismissDisabled(working)
    }

    private var fields: some View {
        VStack(alignment: .leading, spacing: 16) {
            // 운영체제: Windows만 지원합니다.
            HStack(spacing: 12) {
                CreateVMChoiceTile(title: "Windows", icon: "pc", selected: true) {}
                    .accessibilityIdentifier("bridgevm.create.os.windows")
            }

            // 설치 방식
            HStack(spacing: 8) {
                CreateVMMethodChoiceTile(title: "ISO에서 설치", selected: mode == .windowsHVFInstall) {
                    mode = .windowsHVFInstall; autofillWin11()
                }
                .accessibilityIdentifier("bridgevm.create.windows.install")
                CreateVMMethodChoiceTile(title: "설치된 디스크 가져오기", selected: mode == .windowsHVF) {
                    mode = .windowsHVF; isoPath = ""
                }
            }

            if mode == .windowsHVF {
                Text("이미 설치되어 정상 부팅한 Windows ARM 디스크(RAW, QCOW2, Hyper-V의 VHDX)를 가져옵니다. UEFI vars가 없으면 기본 부팅 설정을 만듭니다. QCOW2와 VHDX는 RAW로 변환하고, 원본은 변경하지 않고 라이브러리에 복제합니다.")
                    .font(.callout).foregroundColor(.secondary)
                HStack {
                    Button("설치된 디스크 선택…") { pickHVFTarget() }
                    Text(hvfTargetPath.isEmpty ? "선택된 디스크 없음" : (hvfTargetPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                HStack {
                    Button("UEFI vars 선택… (선택 사항)") { pickHVFVars() }
                    Text(hvfVarsPath.isEmpty ? "선택된 vars 없음" : (hvfVarsPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                Text(HvfWindowsImportNotice.varsHelp)
                    .font(.caption).foregroundColor(.secondary)
                Text(HvfWindowsImportNotice.bitLockerHelp)
                    .font(.caption).foregroundColor(.secondary)
                Text("이번 버전은 3D 가속을 지원하지 않습니다. 기본 디스플레이 구성으로 가져옵니다.")
                    .font(.caption).foregroundColor(.secondary)
            } else {
                CreateWindowsInstallFields(isoPath: isoPath, guestPayloadPath: guestPayloadPath,
                    guestPayloadManifestPath: guestPayloadManifestPath, pickISO: pickISO,
                    pickGuestPayload: pickGuestPayload, pickGuestPayloadManifest: pickGuestPayloadManifest,
                    fetchingDrivers: fetchingDrivers, driverFetchError: driverFetchError,
                    fetchDrivers: fetchGuestDrivers)
            }

            HStack {
                Text("이름").frame(width: 64, alignment: .leading)
                TextField("Windows 11", text: $name)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.create.name")
            }
            if !name.isEmpty, VMLibrary.normalizedVMName(name) == nil {
                Text("이름은 제어문자 없이 1~\(VMLibrary.maximumVMNameCharacters)자이며 파일 ID 제한 안이어야 합니다.")
                    .font(.caption)
                    .foregroundColor(.red)
            }

            HStack {
                Text("저장 위치").frame(width: 64, alignment: .leading)
                Button("폴더 선택…") { pickStorage() }
                Text(storageDir?.path ?? "기본 (라이브러리)")
                    .font(.caption).foregroundColor(.secondary).lineLimit(1).truncationMode(.middle)
                if storageDir != nil {
                    Button { storageDir = nil } label: { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.plain)
                        .accessibilityLabel("저장 위치를 기본 라이브러리로 재설정")
                }
            }

            CreateVMResolutionRow(usesGuestScreen: true,
                                  selection: $resIndex, resolutions: resolutions)

            DisclosureGroup("고급 설정", isExpanded: $showAdvanced) {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text("메모리").frame(width: 64, alignment: .leading)
                        Picker("메모리", selection: $ramMiB) {
                            ForEach(ramOptions, id: \.self) { Text("\($0 / 1024) GiB").tag($0) }
                        }.labelsHidden().frame(width: 150)
                        Spacer()
                    }
                    HStack {
                        Text("CPU").frame(width: 64, alignment: .leading)
                        Stepper("\(cpuCount) 코어", value: $cpuCount, in: 1...maxCPU)
                            .frame(width: 150)
                        Spacer()
                    }
                    if createsFreshDisk {
                        HStack {
                            Text("디스크").frame(width: 64, alignment: .leading)
                            Picker("디스크", selection: $diskGiB) {
                                ForEach(diskOptions, id: \.self) { Text("\($0) GiB").tag($0) }
                            }.labelsHidden().frame(width: 150)
                            Spacer()
                        }
                    }
                    Toggle("공유 네트워크 (NAT)", isOn: $hvfNetwork).font(.callout)
                    Text("호스트: \(String(format: "%.0f", library.hostMemGiB)) GiB · \(library.hostCPU) 코어")
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
        FileSelection.choose(directories: true) { guestPayloadPath = $0.path }
    }

    private func pickGuestPayloadManifest() {
        FileSelection.choose(directories: false, extensions: ["tsv"]) { guestPayloadManifestPath = $0.path }
    }

    /// Only on the user's click: the sheet opens with every source selection empty.
    private func fetchGuestDrivers() {
        guard !fetchingDrivers else { return }
        fetchingDrivers = true
        driverFetchError = ""
        Task.detached {
            let result = Result { try HvfWindowsDriverPayloadFetch.prepare() }
            await MainActor.run {
                fetchingDrivers = false
                switch result {
                case .success(let prepared):
                    guestPayloadPath = prepared.payloadDirectory
                    guestPayloadManifestPath = prepared.manifestPath
                case .failure(let failure):
                    driverFetchError = (failure as? HvfWindowsDriverPayloadFetch.Failure)?.message
                        ?? "드라이버를 준비하지 못했습니다. 다시 시도하세요."
                }
            }
        }
    }

    private func pickHVFTarget() {
        FileSelection.choose(directories: false, extensions: ["raw", "img", "qcow2", "vhdx"]) { hvfTargetPath = $0.path }
    }

    private func pickHVFVars() {
        FileSelection.choose(directories: false, extensions: ["fd", "vars"]) { hvfVarsPath = $0.path }
    }

    var canCreate: Bool {
        guard !working, creationState.permitsCreation, VMLibrary.normalizedVMName(name) != nil else { return false }
        switch mode {
        case .windowsHVF:
            return !hvfTargetPath.isEmpty
        case .windowsHVFInstall:
            return !isoPath.isEmpty && !guestPayloadPath.isEmpty && !guestPayloadManifestPath.isEmpty
        }
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
