extension NativeCLIOptions.Command {
    static func runtime(verb: String, id: String) -> Self {
        verb == "start" ? .start(id) : .stop(id)
    }
}
