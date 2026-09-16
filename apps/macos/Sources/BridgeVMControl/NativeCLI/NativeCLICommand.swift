extension NativeCLIOptions.Command {
    static func parse(positionals: [String], help: Bool) throws -> Self {
        if positionals.isEmpty || positionals == ["list"] { return .list }
        guard let verb = positionals.first, ["inspect", "readiness", "status", "start", "stop"].contains(verb),
              positionals.count == 2 || (help && positionals.count == 1) else {
            throw NativeCLIError.invalid("Expected 'list', 'inspect ID', 'readiness ID', 'status ID', 'start ID' or 'stop ID'. Use --cli --help.")
        }
        let id = positionals.count == 2 ? positionals[1] : ""
        guard (help && id.isEmpty && positionals.count == 1) || NativeLibraryReader.isCanonicalID(id) else {
            throw NativeCLIError.invalid("Use an exact VM ID; paths and noncanonical IDs are refused.")
        }
        return make(verb: verb, id: id)
    }
}
