enum HvfConnectionState: Equatable {
    case stopped
    case booting
    case connected(host: String)
    case stopping
    case timedOut
}
