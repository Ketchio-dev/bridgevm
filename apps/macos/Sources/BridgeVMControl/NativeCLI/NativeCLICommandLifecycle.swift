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
