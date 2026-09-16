extension NativeCLIOptions.Command {
    static func make(verb: String, id: String) -> Self {
        selected(verb: verb, id: id)
    }
}
