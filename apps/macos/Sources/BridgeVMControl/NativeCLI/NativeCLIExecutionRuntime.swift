extension NativeCLI {
    static func executeRuntime(_ options: NativeCLIOptions) throws -> Int32 {
        switch options.command {
        case .start(let id):
            let result = NativeCLIRuntimeStart.run(rootURL: options.libraryRoot, id: id)
            try output(result, json: options.json, text: result.text)
            return result.started ? 0 : 1
        case .stop(let id):
            let result = NativeCLIRuntimeStop.run(rootURL: options.libraryRoot, id: id)
            try output(result, json: options.json, text: result.text)
            return result.complete ? 0 : 1
        default: throw NativeCLIError.invalid("Expected a runtime command.")
        }
    }
}
