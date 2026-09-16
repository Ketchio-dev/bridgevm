import Foundation

extension NativeCLI {
    static func execute(_ options: NativeCLIOptions) throws -> Int32 {
        switch options.command {
        case .stop(let id):
            let result = NativeCLIRuntimeStop.run(rootURL: options.libraryRoot, id: id)
            try output(result, json: options.json, text: result.text)
            return result.complete ? 0 : 1
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
    }
}
