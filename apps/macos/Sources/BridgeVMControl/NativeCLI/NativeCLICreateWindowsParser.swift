import Darwin
import Foundation

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
        var json = false, help = false, network = true, index = 0
        var seen = Set<String>()
        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "create-windows": try once("command", seen: &seen)
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
            memoryMiB: memory, cpuCount: cpus, resolution: resolution, networkEnabled: network)
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
