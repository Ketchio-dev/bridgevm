import Foundation

enum NativeCLI {
    static func run(arguments: [String]) -> Int32 {
        do {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            if options.showHelp {
                write(help + "\n", to: .standardOutput)
                return 0
            }
            switch options.command {
            case .status(let id):
                let snapshot = NativeCLIRuntimeStatus.snapshot(rootURL: options.libraryRoot, id: id)
                try output(snapshot, json: options.json, text: snapshot.text)
                return snapshot.complete ? 0 : 1
            case .readiness(let id):
                let snapshot = try NativeCLIReadiness.snapshot(rootURL: options.libraryRoot, id: id)
                try output(snapshot, json: options.json, text: render(snapshot))
                return snapshot.launchReady ? 0 : 1
            case .list, .inspect:
                let id: String?
                if case .inspect(let value) = options.command { id = value } else { id = nil }
                let snapshot = try NativeLibraryReader.snapshot(rootURL: options.libraryRoot, id: id)
                try output(snapshot, json: options.json, text: render(snapshot))
                return snapshot.complete ? 0 : 1
            }
        } catch {
            write("BridgeVM: \(error.localizedDescription)\n", to: .standardError)
            if case NativeCLIError.invalid = error { return 2 }
            return 1
        }
    }
}
