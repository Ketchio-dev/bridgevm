/// A sent press may have inserted even while its receipt is still unknown.
struct HvfPointerHeldState {
    private var buttons = 0
    private var location: String?
    var release: String? { buttons == 0 ? nil : location.map { "releaseall:" + $0 } }

    mutating func sent(_ event: HvfOrderedInputQueue.Event) {
        guard case let .pointer(command) = event else { return }
        let fields = command.split(separator: ":", maxSplits: 1)
        guard fields.count == 2 else { return }
        switch fields[0] {
        case "press", "click": buttons |= 1
        case "rightpress", "rightclick": buttons |= 2
        default: break
        }
        guard buttons != 0, fields[0] != "wheel" else { return }
        location = fields[0] == "scroll" ? fields[1].split(separator: "@").last.map(String.init) : String(fields[1])
    }

    mutating func inserted(_ event: HvfOrderedInputQueue.Event) {
        guard case let .pointer(command) = event else { return }
        switch command.split(separator: ":", maxSplits: 1).first {
        case "release", "click": buttons &= ~1
        case "rightrelease", "rightclick": buttons &= ~2
        case "releaseall": buttons = 0
        default: break
        }
        if buttons == 0 { location = nil }
    }
}
