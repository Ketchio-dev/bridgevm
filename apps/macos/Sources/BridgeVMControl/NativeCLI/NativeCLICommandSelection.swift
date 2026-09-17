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
