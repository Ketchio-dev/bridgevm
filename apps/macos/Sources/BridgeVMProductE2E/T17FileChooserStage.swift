enum T17FileChooserStage {
    static func run<Value>(_ name: String, _ operation: () throws -> Value) throws -> Value {
        do { return try operation() }
        catch let blocker as T17Blocker where blocker.code == "input-selection-failed" {
            throw T17FileChooser.failure("stage=\(name); \(blocker.detail)")
        }
    }
}
