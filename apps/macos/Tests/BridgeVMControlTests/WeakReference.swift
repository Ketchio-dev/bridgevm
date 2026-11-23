/// A weak reference bound once, for tests that check an object is released. Swift 6.2 spells
/// this `weak let`; Swift 6.1, the toolchain of the macos-15 CI runner, rejects that.
struct WeakReference<Object: AnyObject> {
    weak var object: Object?
    init(_ object: Object?) { self.object = object }
}
