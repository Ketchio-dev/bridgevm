import Foundation

enum NativeCLI {
    static func run(arguments: [String]) -> Int32 {
        do {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            if options.showHelp {
                write(help + "\n", to: .standardOutput)
                return 0
            }
            return try execute(options)
        } catch {
            write("BridgeVM: \(error.localizedDescription)\n", to: .standardError)
            if case NativeCLIError.invalid = error { return 2 }
            return 1
        }
    }
}
