import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif

// MARK: - Create sheet (gallery)
struct CreateVMSheet: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.dismiss) var dismiss
    @State var osFamily: OSFamily = .windows
    @State var mode: Mode = .windowsHVFInstall
    @State var name = ""
    @State var isoPath: String = ""
    @State var guestPayloadPath: String = ""
    @State var guestPayloadManifestPath: String = ""
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
    let resolutions = [(1280, 800), (1440, 900), (1920, 1080), (2560, 1440)]
    enum OSFamily: Equatable { case windows, linux }
    enum Mode: Equatable {
        case ubuntu, iso, windows, windowsHVF, windowsHVFInstall
        var family: OSFamily {
            switch self {
            case .ubuntu, .iso: return .linux
            case .windows, .windowsHVF, .windowsHVFInstall: return .windows
            }
        }
    }
    var template: VMConfig? {
        library.vms.first { $0.backendKind == "fast-vz" && ($0.bootMode ?? "direct-kernel") == "direct-kernel" }
            ?? library.vms.first
    }
    /// Memory choices in MiB, capped just under host physical RAM.
    private var ramOptions: [Int] {
        let hostMiB = Int(library.hostMemGiB * 1024)
        let ceiling = max(4096, hostMiB - 4096) // leave headroom for macOS
        return [2048, 4096, 6144, 8192, 12288, 16384, 24576, 32768].filter { $0 <= ceiling }
    }
    private var maxCPU: Int { max(1, library.hostCPU - 1) }
    /// Modes that allocate a brand-new blank disk the user can size.
    private var createsFreshDisk: Bool {
        mode == .iso || mode == .windows || mode == .windowsHVFInstall
    }

    /// HVF-engine Windows modes where the guest NIC can be toggled (Fast VZ
    /// requires NAT, so Linux never shows the toggle).
    private var isHVFWindows: Bool {
        mode == .windowsHVFInstall || mode == .windowsHVF
    }

    private var diskOptions: [Int] {
        osFamily == .windows ? [64, 96, 128, 256, 512] : [20, 40, 64, 96, 128, 256]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("새 VM 만들기").font(.title2.bold()).padding(20)
            Divider()
            ScrollView { fields.padding(20).disabled(working) }
            Divider()
            CreateVMResourceSummary(cpuCount: cpuCount, ramMiB: ramMiB,
                                    diskGiB: createsFreshDisk ? diskGiB : nil)
                .padding(.horizontal, 20).padding(.top, 12)
            CreateVMCreationFooter(working: working, error: error, failureCode: creationFailureCode,
                                   canCreate: canCreate, cancel: { dismiss() }, create: create)
        }
        .frame(width: 480, height: 640)
        .interactiveDismissDisabled(working)
    }

    private var fields: some View {
        VStack(alignment: .leading, spacing: 16) {
            // 1단계: 운영체제 선택 (Windows / Linux)
            HStack(spacing: 12) {
                CreateVMChoiceTile(title: "Windows", icon: "pc", selected: osFamily == .windows) { selectFamily(.windows) }
                    .accessibilityIdentifier("bridgevm.create.os.windows")
                CreateVMChoiceTile(title: "Linux", icon: "terminal", selected: osFamily == .linux) { selectFamily(.linux) }
            }

            // 2단계: 세부 설치 방식
            HStack(spacing: 8) {
                if osFamily == .windows {
                    CreateVMMethodChoiceTile(title: "ISO에서 설치", selected: mode == .windowsHVFInstall) {
                        mode = .windowsHVFInstall; autofillWin11()
                    }
                    .accessibilityIdentifier("bridgevm.create.windows.install")
                    CreateVMMethodChoiceTile(title: "설치된 디스크 가져오기", selected: mode == .windowsHVF) {
                        mode = .windowsHVF; isoPath = ""
                    }
                    CreateVMMethodChoiceTile(title: "QEMU 호환", selected: mode == .windows) {
                        mode = .windows; autofillWin11()
                    }
                } else {
                    CreateVMMethodChoiceTile(title: "Ubuntu 즉시 복제", selected: mode == .ubuntu) { mode = .ubuntu }
                    CreateVMMethodChoiceTile(title: "Linux ISO 설치", selected: mode == .iso) { mode = .iso }
                }
            }

            if mode == .ubuntu {
                Text("기본 Ubuntu 데스크톱을 즉시 복제합니다 (APFS 클론, 추가 용량 없음).")
                    .font(.callout).foregroundColor(.secondary)
            } else if mode == .windowsHVF {
                Text("이미 설치되어 정상 부팅한 Windows ARM RAW 디스크와 그 부팅에 사용한 UEFI vars를 가져옵니다. 원본은 변경하지 않고 라이브러리에 복제합니다.")
                    .font(.callout).foregroundColor(.secondary)
                HStack {
                    Button("설치된 RAW 선택…") { pickHVFTarget() }
                    Text(hvfTargetPath.isEmpty ? "선택된 디스크 없음" : (hvfTargetPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                HStack {
                    Button("UEFI vars 선택…") { pickHVFVars() }
                    Text(hvfVarsPath.isEmpty ? "선택된 vars 없음" : (hvfVarsPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                Text("3D 드라이버 주입은 서명 provenance 검증기가 없어 사용할 수 없습니다. 3D 주입 없이 가져옵니다.")
                    .font(.caption).foregroundColor(.secondary)
            } else if mode == .windowsHVFInstall {
                CreateWindowsInstallFields(isoPath: isoPath, guestPayloadPath: guestPayloadPath,
                    guestPayloadManifestPath: guestPayloadManifestPath, pickISO: pickISO,
                    pickGuestPayload: pickGuestPayload, pickGuestPayloadManifest: pickGuestPayloadManifest)
            } else {
                Text(mode == .windows
                     ? "Windows 11 ARM ISO를 선택하면 QEMU + TPM 2.0으로 설치 마법사를 부팅합니다."
                     : "원하는 리눅스 배포판 ISO를 선택하면 EFI로 부팅해 설치하는 새 VM을 만듭니다.")
                    .font(.callout).foregroundColor(.secondary)
                HStack {
                    Button("ISO 선택…") { pickISO() }
                    Text(isoPath.isEmpty ? "선택된 ISO 없음" : (isoPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
            }

            HStack {
                Text("이름").frame(width: 64, alignment: .leading)
                TextField(osFamily == .windows ? "Windows 11" : "Ubuntu 2", text: $name)
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

            HStack {
                Text("해상도").frame(width: 64, alignment: .leading)
                Picker("해상도", selection: $resIndex) {
                    ForEach(0..<resolutions.count, id: \.self) { i in
                        Text("\(resolutions[i].0)×\(resolutions[i].1)").tag(i)
                    }
                }.labelsHidden().frame(width: 150)
                Spacer()
            }

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
                    if isHVFWindows {
                        Toggle("공유 네트워크 (NAT)", isOn: $hvfNetwork).font(.callout)
                    } else {
                        HStack {
                            Text("네트워크").frame(width: 64, alignment: .leading)
                            Text("공유 (NAT) — 고정").font(.caption).foregroundColor(.secondary)
                            Spacer()
                        }
                    }
                    Text("호스트: \(String(format: "%.0f", library.hostMemGiB)) GiB · \(library.hostCPU) 코어")
                        .font(.caption).foregroundColor(.secondary)
                }
                .padding(.top, 8)
            }
            .font(.callout)


        }
    }

    /// Switch OS family and reset to that family's default install method.
    private func selectFamily(_ family: OSFamily) {
        guard osFamily != family else { return }
        osFamily = family
        switch family {
        case .windows:
            mode = .windowsHVFInstall
            ramMiB = clampRam(6144)
            diskGiB = 64
            autofillWin11()
        case .linux:
            mode = .ubuntu
            ramMiB = clampRam(4096)
            diskGiB = 40
        }
    }

    /// Snap a desired RAM value to the nearest available host-capped option.
    private func clampRam(_ desired: Int) -> Int {
        let options = ramOptions
        if options.contains(desired) { return desired }
        return options.last ?? desired
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

    private func pickHVFTarget() {
        FileSelection.choose(directories: false, extensions: ["raw", "img"]) { hvfTargetPath = $0.path }
    }

    private func pickHVFVars() {
        FileSelection.choose(directories: false, extensions: ["fd", "vars"]) { hvfVarsPath = $0.path }
    }

    private var canCreate: Bool {
        guard !working, VMLibrary.normalizedVMName(name) != nil else { return false }
        switch mode {
        case .windowsHVF:
            return !hvfTargetPath.isEmpty && !hvfVarsPath.isEmpty
        case .windowsHVFInstall:
            return !isoPath.isEmpty && !guestPayloadPath.isEmpty && !guestPayloadManifestPath.isEmpty
        case .iso, .windows:
            return !isoPath.isEmpty && template != nil
        case .ubuntu:
            return template != nil
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
