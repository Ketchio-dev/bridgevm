import Foundation

struct NativeCLISnapshotExportOptions: Equatable {
    let id: String
    let output: URL
}

enum NativeCLISnapshotExportParser {
    static func selectsCommand(_ arguments: [String]) -> Bool {
        var skipValue = false
        for argument in arguments {
            if skipValue { skipValue = false; continue }
            if argument == "--library" { skipValue = true; continue }
            if argument.hasPrefix("-") { continue }
            return argument == "snapshot-export"
        }
        return false
    }

    static func parse(arguments: [String], defaultLibrary: URL) throws -> NativeCLIOptions {
        var positionals: [String] = [], library: URL?
        var json = false, help = false, index = 0
        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "--library":
                guard library == nil, index + 1 < arguments.count else { throw invalid("--library requires one absolute path and may appear only once.") }
                index += 1
                library = try absolute(arguments[index], label: "--library")
            case "--json":
                guard !json else { throw invalid("--json may appear only once.") }
                json = true
            case "--help", "-h": help = true
            default:
                guard !argument.hasPrefix("-") else { throw invalid("Unknown option: \(argument). Use --cli --help.") }
                positionals.append(argument)
            }
            index += 1
        }
        if help, positionals == ["snapshot-export"] {
            return NativeCLIOptions(command: .snapshotExport(.init(id: "", output: URL(fileURLWithPath: "/"))), libraryRoot: library ?? defaultLibrary, json: json, showHelp: true)
        }
        guard positionals.count == 3, positionals[0] == "snapshot-export" else { throw invalid("Expected snapshot-export ID OUTPUT. Use --cli --help.") }
        guard NativeLibraryReader.isCanonicalID(positionals[1]) else { throw invalid("Use an exact VM ID; paths and noncanonical IDs are refused.") }
        let output = try absolute(positionals[2], label: "OUTPUT")
        guard output.path != "/" else { throw invalid("OUTPUT must name an export directory.") }
        return NativeCLIOptions(command: .snapshotExport(.init(id: positionals[1], output: output)), libraryRoot: library ?? defaultLibrary, json: json, showHelp: false)
    }

    private static func absolute(_ path: String, label: String) throws -> URL {
        guard path.hasPrefix("/"), !(path as NSString).pathComponents.contains("..") else { throw invalid("\(label) requires an absolute path without '..'.") }
        return URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
    }

    private static func invalid(_ message: String) -> NativeCLIError { .invalid(message) }
}
