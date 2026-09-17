extension NativeCLI {
    static func execute(_ options: NativeCLIOptions) throws -> Int32 {
        switch options.command {
        case .start, .stop: return try executeRuntime(options)
        case .install, .installStatus, .installCancel: return try executeInstall(options)
        case .snapshotCreate, .snapshotRestore: return try executeSnapshot(options)
        case .status(let id):
            let snapshot = NativeCLIRuntimeStatus.snapshot(rootURL: options.libraryRoot, id: id)
            try output(snapshot, json: options.json, text: snapshot.text)
            return snapshot.complete ? 0 : 1
        case .readiness(let id):
            let snapshot = try NativeCLIReadiness.snapshot(rootURL: options.libraryRoot, id: id)
            try output(snapshot, json: options.json, text: render(snapshot))
            return snapshot.launchReady ? 0 : 1
        case .list, .inspect, .createWindows:
            if case .createWindows = options.command { return try executeCreateWindows(options) }
            let id: String?
            if case .inspect(let value) = options.command { id = value } else { id = nil }
            let snapshot = try NativeLibraryReader.snapshot(rootURL: options.libraryRoot, id: id)
            try output(snapshot, json: options.json, text: render(snapshot)); return snapshot.complete ? 0 : 1
        }
    }
}
