import Foundation

struct NativeCLIOptions: Equatable {
    enum Command: Equatable { case list, inspect(String), readiness(String), status(String), start(String), stop(String), install(String), installStatus(String), installCancel(String), snapshotCreate(String), snapshotRestore(String), snapshotExport(NativeCLISnapshotExportOptions), createWindows(NativeCLICreateWindowsOptions) }
    let command: Command
    let libraryRoot: URL
    let json: Bool
    let showHelp: Bool

    static func parse(arguments: [String], defaultLibrary: URL = VMLibrary.root) throws -> Self {
        if NativeCLISnapshotExportParser.selectsCommand(arguments) { return try NativeCLISnapshotExportParser.parse(arguments: arguments, defaultLibrary: defaultLibrary) }
        if NativeCLICreateWindowsParser.selectsCommand(arguments) { return try NativeCLICreateWindowsParser.parse(arguments: arguments, defaultLibrary: defaultLibrary) }
        var positionals: [String] = []
        var library: URL?
        var json = false
        var help = false
        var index = 0
        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "--library":
                guard library == nil, index + 1 < arguments.count else {
                    throw NativeCLIError.invalid("--library requires one absolute path and may appear only once.")
                }
                index += 1
                let path = arguments[index]
                guard path.hasPrefix("/"), !(path as NSString).pathComponents.contains("..") else {
                    throw NativeCLIError.invalid("--library requires an absolute path without '..'.")
                }
                library = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
            case "--json":
                guard !json else { throw NativeCLIError.invalid("--json may appear only once.") }
                json = true
            case "--help", "-h": help = true
            default:
                guard !argument.hasPrefix("-") else {
                    throw NativeCLIError.invalid("Unknown option: \(argument). Use --cli --help.")
                }
                positionals.append(argument)
            }
            index += 1
        }
        return Self(command: try Command.parse(positionals: positionals, help: help), libraryRoot: library ?? defaultLibrary, json: json, showHelp: help || arguments.isEmpty)
    }
}
