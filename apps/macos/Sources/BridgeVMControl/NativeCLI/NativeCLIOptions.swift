import Foundation

struct NativeCLIOptions: Equatable {
    enum Command: Equatable { case list, inspect(String), readiness(String), status(String) }
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
        let command = try Command.parse(positionals: positionals, help: help)
        return Self(command: command, libraryRoot: library ?? defaultLibrary,
                    json: json, showHelp: help || arguments.isEmpty)
    }
}
