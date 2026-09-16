extension NativeCLIOptions.Command {
    static func make(verb: String, id: String) -> Self {
        switch verb {
        case "inspect": return .inspect(id)
        case "readiness": return .readiness(id)
        case "stop": return .stop(id)
        default: return .status(id)
        }
    }
}
