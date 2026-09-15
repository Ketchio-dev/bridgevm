import Foundation

struct NativeCLIOptions: Equatable {
    enum Command: Equatable { case list, inspect(String), readiness(String) }
    let command: Command
    let libraryRoot: URL
    let json: Bool
    let showHelp: Bool

    static func parse(arguments: [String], defaultLibrary: URL = VMLibrary.root) throws -> Self {
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
        let command: Command
        switch positionals {
        case [], ["list"]: command = .list
        case let values where help && values.count == 1 && ["inspect", "readiness"].contains(values[0]):
            command = values[0] == "inspect" ? .inspect("") : .readiness("")
        case let values where values.count == 2 && ["inspect", "readiness"].contains(values[0]):
            guard NativeLibraryReader.isCanonicalID(values[1]) else {
                throw NativeCLIError.invalid("Use the exact VM ID from 'list'; paths and noncanonical IDs are refused.")
            }
            command = values[0] == "inspect" ? .inspect(values[1]) : .readiness(values[1])
        default: throw NativeCLIError.invalid("Expected 'list', 'inspect ID' or 'readiness ID'. Use --cli --help.")
        }
        return Self(command: command, libraryRoot: library ?? defaultLibrary,
                    json: json, showHelp: help || arguments.isEmpty)
    }
}
