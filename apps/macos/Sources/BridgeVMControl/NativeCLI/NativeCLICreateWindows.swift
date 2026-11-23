import Foundation
import Darwin

enum NativeCLICreateWindows {
    typealias Creator = (NativeCLICreateWindowsOptions, URL) -> HVFCreationOutcome?
    typealias Reader = (URL, String) throws -> VMConfig

    static func create(_ request: NativeCLICreateWindowsOptions, libraryRoot: URL,
                       creator: Creator = createSavedVM,
                       reader: Reader = NativeLibraryReader.readConfig) throws -> NativeCLICreateWindowsOutput {
        let created = try requireCreated(creator(request, libraryRoot))
        let saved = try reader(libraryRoot, created.slug)
        guard saved == created, saved.backendKind == "hvf-engine", saved.bootMode == "windows-hvf",
              saved.installPending == true, saved.displayName == request.name,
              saved.cpuCount == request.cpuCount, saved.memMiB == request.memoryMiB,
              saved.displayWidth == request.resolution.width, saved.displayHeight == request.resolution.height,
              saved.networkEnabled == request.networkEnabled,
              let pending = HvfWindowsInstallRequest.load(bundlePath: saved.bundlePath),
              pending.diskGiB == request.diskGiB, pending.isoSHA256 != nil,
              (request.guestPayload == nil) == (pending.guestPayloadIdentity == nil) else {
            throw NativeCLIError.unavailable("Saved VM registration did not match the accepted creation request.")
        }
        return NativeCLICreateWindowsOutput(id: saved.slug, displayName: saved.displayName,
            backendKind: saved.backendKind, bootMode: saved.bootMode ?? "", installPending: true,
            cpuCount: request.cpuCount, memoryMiB: request.memoryMiB, diskGiB: request.diskGiB,
            resolution: request.resolution.text, networkEnabled: request.networkEnabled,
            guestDriversSealed: pending.guestPayloadIdentity != nil,
            configurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: saved))
    }


}

extension NativeCLI {
    static func executeCreateWindows(_ options: NativeCLIOptions) throws -> Int32 {
        guard case .createWindows(var request) = options.command else {
            throw NativeCLIError.invalid("Expected create-windows command.")
        }
        if request.fetchDrivers {
            write("BridgeVM: preparing the pinned Red Hat virtio-win drivers (about 95 MB on first use)...\n", to: .standardError)
            do { request.guestPayload = try HvfWindowsDriverPayloadFetch.prepare() } catch {
                throw NativeCLIError.unavailable((error as? HvfWindowsDriverPayloadFetch.Failure)?.message
                    ?? "드라이버를 준비하지 못했습니다. 다시 시도하세요.")
            }
        }
        let result = try NativeCLICreateWindows.create(request, libraryRoot: options.libraryRoot)
        try output(result, json: options.json, text: result.text)
        return 0
    }
}

extension NativeCLICreateWindows {
    static func requireCreated(_ outcome: HVFCreationOutcome?) throws -> VMConfig {
        switch outcome {
        case .created(let config): return config
        case .publishedButUnsynced(let config, _):
            throw NativeCLIError.unavailable("VM files and registration were preserved, but durable saving was not confirmed. Do not recreate; inspect the library entry: \(config.slug)")
        case .refused(let message):
            throw NativeCLIError.unavailable(message)
        case nil:
            throw NativeCLIError.unavailable("VM creation did not publish a confirmed registration.")
        }
    }

    static func createSavedVM(_ request: NativeCLICreateWindowsOptions,
                                      _ libraryRoot: URL) -> HVFCreationOutcome? {
        VMLibrary.createWindowsHVFInstall(name: request.name, isoPath: request.isoPath,
            diskGiB: request.diskGiB, injectViogpu3d: false, driverPackageDir: nil,
            guestPayloadDirectory: request.guestPayload?.payloadDirectory,
            guestPayloadManifest: request.guestPayload?.manifestPath,
            storageDir: nil, width: request.resolution.width, height: request.resolution.height,
            memMiB: request.memoryMiB, cpuCount: request.cpuCount,
            networkEnabled: request.networkEnabled, libraryRoot: libraryRoot)
    }
}

enum NativeCLICreateWindowsGuidance {
    static let limitation = "CLI-created registrations lack the required signed ARM64 guest driver payload; installation is blocked."
    static let setup = "For installation, create a new VM in the app with the ISO, signed payload and manifest. Existing CLI entries cannot be completed through the app or CLI."
    static let help = ["Without --fetch-drivers: " + limitation, setup,
        "With --fetch-drivers, the pinned Microsoft-signed Red Hat virtio-win storage,",
        "serial and network drivers (about 95 MB) are downloaded once from the official",
        "source, checked against their pinned SHA-256 and sealed with the VM, so the",
        "install command can complete it."].joined(separator: "\n      ")

    static func text(id: String) -> String {
        "No installation or guest boot was run.\n"
            + limitation + "\n" + setup + "\n"
            + "Inspect the saved registration: bridgevm app inspect \(id)\n"
    }

    static func sealedText(id: String) -> String {
        "No installation or guest boot was run.\n"
            + "Signed ARM64 guest drivers (Red Hat virtio-win) are sealed in the VM bundle.\n"
            + "Next: bridgevm app install \(id)\n"
    }
}

struct NativeCLICreateWindowsOutput: Encodable, Equatable {
    let schema = "bridgevm.app-create-windows.v1"
    let id: String
    let displayName: String
    let backendKind: String
    let bootMode: String
    let installPending: Bool
    let cpuCount: Int
    let memoryMiB: Int
    let diskGiB: Int
    let resolution: String
    let networkEnabled: Bool
    /// The pinned virtio-win payload from `--fetch-drivers` is sealed in the bundle.
    let guestDriversSealed: Bool
    let configurationDigest: String

    var text: String {
        "Created pending Windows VM \(id) (\(displayName)).\n" +
        "Saved resources: \(cpuCount) CPU, \(memoryMiB) MiB, \(diskGiB) GiB, \(resolution).\n" +
        (guestDriversSealed ? NativeCLICreateWindowsGuidance.sealedText(id: id) : NativeCLICreateWindowsGuidance.text(id: id))
    }
}

struct NativeCLICreateWindowsOptions: Equatable {
    struct Resolution: Equatable {
        let width: Int
        let height: Int
        var text: String { "\(width)x\(height)" }
    }

    let name: String
    let isoPath: String
    let diskGiB: Int
    let memoryMiB: Int
    let cpuCount: Int
    let resolution: Resolution
    let networkEnabled: Bool
    /// `--fetch-drivers`: prepare the pinned virtio-win payload and seal it with the VM.
    var fetchDrivers = false
    /// The prepared payload, filled in just before creation.
    var guestPayload: HvfWindowsDriverPayloadFetch.Prepared? = nil

    static let diskChoices = [64, 96, 128, 256, 512]
    static let memoryChoices = [2_048, 4_096, 6_144, 8_192, 12_288, 16_384, 24_576, 32_768]
    static let resolutions = [
        "1280x800": Resolution(width: 1280, height: 800),
        "1440x900": Resolution(width: 1440, height: 900),
        "1920x1080": Resolution(width: 1920, height: 1080),
        "2560x1440": Resolution(width: 2560, height: 1440),
    ]
}

enum NativeCLICreateWindowsParser {
    static func selectsCommand(_ arguments: [String]) -> Bool {
        var skipValue = false
        for argument in arguments {
            if skipValue { skipValue = false; continue }
            if argument == "--library" { skipValue = true; continue }
            if ["--json", "--help", "-h"].contains(argument) { continue }
            return argument == "create-windows"
        }
        return false
    }

    static func parse(arguments: [String], defaultLibrary: URL,
                      hostCPU: Int = ProcessInfo.processInfo.activeProcessorCount) throws -> NativeCLIOptions {
        var name: String?, iso: String?, library: URL?
        var disk = 64, memory = 6_144, cpus = 4
        var resolution = NativeCLICreateWindowsOptions.resolutions["1440x900"]!
        var json = false, help = false, network = true, fetchDrivers = false, index = 0
        var seen = Set<String>()
        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "create-windows" where !seen.contains("command"): try once("command", seen: &seen)
            case "--iso": iso = try value(argument, arguments, index: &index, seen: &seen)
            case "--disk-gib": disk = try integer(argument, arguments, index: &index, seen: &seen)
            case "--memory-mib": memory = try integer(argument, arguments, index: &index, seen: &seen)
            case "--cpus": cpus = try integer(argument, arguments, index: &index, seen: &seen)
            case "--resolution":
                let text = try value(argument, arguments, index: &index, seen: &seen)
                guard let selected = NativeCLICreateWindowsOptions.resolutions[text] else {
                    throw NativeCLIError.invalid("Unsupported --resolution value.")
                }
                resolution = selected
            case "--library":
                let path = try value(argument, arguments, index: &index, seen: &seen)
                guard absolute(path) else { throw NativeCLIError.invalid("--library requires an absolute path without '..'.") }
                library = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
            case "--json": try once(argument, seen: &seen); json = true
            case "--no-network": try once(argument, seen: &seen); network = false
            case "--fetch-drivers": try once(argument, seen: &seen); fetchDrivers = true
            case "--help", "-h": try once("help", seen: &seen); help = true
            default:
                guard !argument.hasPrefix("-"), name == nil else {
                    throw NativeCLIError.invalid("Unknown or duplicate create-windows argument. Use --cli --help.")
                }
                name = argument
            }
            index += 1
        }
        guard seen.contains("command") else { throw NativeCLIError.invalid("Expected create-windows command.") }
        if help {
            let placeholder = NativeCLICreateWindowsOptions(name: "help", isoPath: "/help", diskGiB: disk,
                memoryMiB: memory, cpuCount: cpus, resolution: resolution, networkEnabled: network)
            return .init(command: .createWindows(placeholder), libraryRoot: library ?? defaultLibrary,
                         json: json, showHelp: true)
        }
        guard let name, VMLibrary.normalizedVMName(name) == name,
              let iso, absolute(iso), readableRegularFileWithoutSymlink(iso),
              NativeCLICreateWindowsOptions.diskChoices.contains(disk),
              NativeCLICreateWindowsOptions.memoryChoices.contains(memory),
              (1...max(1, hostCPU - 1)).contains(cpus) else {
            throw NativeCLIError.invalid("Invalid create-windows name, ISO, or resource value.")
        }
        let request = NativeCLICreateWindowsOptions(name: name, isoPath: iso, diskGiB: disk,
            memoryMiB: memory, cpuCount: cpus, resolution: resolution, networkEnabled: network,
            fetchDrivers: fetchDrivers)
        return .init(command: .createWindows(request), libraryRoot: library ?? defaultLibrary,
                     json: json, showHelp: false)
    }

    private static func value(_ flag: String, _ arguments: [String], index: inout Int,
                              seen: inout Set<String>) throws -> String {
        try once(flag, seen: &seen)
        guard index + 1 < arguments.count else { throw NativeCLIError.invalid("\(flag) requires one value.") }
        index += 1
        return arguments[index]
    }

    private static func integer(_ flag: String, _ arguments: [String], index: inout Int,
                                seen: inout Set<String>) throws -> Int {
        guard let result = Int(try value(flag, arguments, index: &index, seen: &seen)) else {
            throw NativeCLIError.invalid("\(flag) requires an integer.")
        }
        return result
    }

    private static func once(_ key: String, seen: inout Set<String>) throws {
        guard seen.insert(key).inserted else { throw NativeCLIError.invalid("Duplicate option: \(key).") }
    }

    private static func absolute(_ path: String) -> Bool {
        path.hasPrefix("/") && !(path as NSString).pathComponents.contains("..")
    }

    private static func readableRegularFileWithoutSymlink(_ path: String) -> Bool {
        var metadata = stat()
        return lstat(path, &metadata) == 0 && metadata.st_mode & S_IFMT == S_IFREG
            && FileManager.default.isReadableFile(atPath: path)
    }
}
