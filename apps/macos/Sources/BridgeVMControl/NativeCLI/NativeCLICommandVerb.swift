extension NativeCLIOptions.Command {
    static func make(verb: String, id: String) -> Self {
        switch verb {
        case "inspect": return .inspect(id)
        case "readiness": return .readiness(id)
        case "start", "stop": return runtime(verb: verb, id: id)
        default: return .status(id)
        }
    }
}
