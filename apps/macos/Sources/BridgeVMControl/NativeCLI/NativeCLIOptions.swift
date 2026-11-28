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
                    throw NativeCLIError.invalid("Unknown option: \(argument). Run `bridgevm app --help`.")
                }
                positionals.append(argument)
            }
            index += 1
        }
        return Self(command: try Command.parse(positionals: positionals, help: help), libraryRoot: library ?? defaultLibrary, json: json, showHelp: help || arguments.isEmpty)
    }
}

extension NativeCLIOptions.Command {
    static func parse(positionals: [String], help: Bool) throws -> Self {
        if positionals.isEmpty || positionals == ["list"] { return .list }
        guard let verb = positionals.first, ["inspect", "readiness", "status", "start", "stop", "install", "install-status", "install-cancel", "snapshot-create", "snapshot-restore"].contains(verb),
              positionals.count == 2 || (help && positionals.count == 1) else {
            throw NativeCLIError.invalid("Expected a supported command and a VM ID. Run `bridgevm app --help`.")
        }
        let id = positionals.count == 2 ? positionals[1] : ""
        guard (help && id.isEmpty && positionals.count == 1) || NativeLibraryReader.isCanonicalID(id) else {
            throw NativeCLIError.invalid(NativeLibraryReader.notAnID(id))
        }
        return make(verb: verb, id: id)
    }
}

extension NativeCLIOptions.Command {
    static func make(verb: String, id: String) -> Self {
        selected(verb: verb, id: id)
    }
}

extension NativeCLIOptions.Command {
    static func selected(verb: String, id: String) -> Self {
        switch verb {
        case "inspect": return .inspect(id)
        case "readiness": return .readiness(id)
        case "install", "install-status", "install-cancel", "snapshot-create", "snapshot-restore":
            return lifecycle(verb: verb, id: id)
        case "start", "stop": return runtime(verb: verb, id: id)
        default: return .status(id)
        }
    }
}

extension NativeCLIOptions.Command {
    static func lifecycle(verb: String, id: String) -> Self {
        switch verb {
        case "install": return .install(id)
        case "install-status": return .installStatus(id)
        case "install-cancel": return .installCancel(id)
        case "snapshot-create": return .snapshotCreate(id)
        default: return .snapshotRestore(id)
        }
    }
}

extension NativeCLIOptions.Command {
    static func runtime(verb: String, id: String) -> Self {
        verb == "start" ? .start(id) : .stop(id)
    }
}
