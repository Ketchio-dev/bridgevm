extension NativeCLIOptions.Command {
    static func selected(verb: String, id: String) -> Self {
        switch verb {
        case "inspect": return .inspect(id)
        case "readiness": return .readiness(id)
        case "install": return .install(id)
        case "install-status": return .installStatus(id)
        case "install-cancel": return .installCancel(id)
        case "start", "stop": return runtime(verb: verb, id: id)
        default: return .status(id)
        }
    }
}
