extension NativeCLI {
    static func executeInstall(_ options: NativeCLIOptions) throws -> Int32 {
        let id: String, operation: NativeInstallControlRequest.Operation
        switch options.command {
        case let .install(value): id = value; operation = .install
        case let .installStatus(value): id = value; operation = .installStatus
        case let .installCancel(value): id = value; operation = .installCancel
        default: throw NativeCLIError.invalid("Expected an install command.")
        }
        let result = NativeCLIInstall.run(rootURL: options.libraryRoot, id: id, operation: operation)
        try output(result, json: options.json, text: result.text)
        return result.complete ? 0 : 1
    }
}
