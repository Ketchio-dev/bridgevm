enum NativeCLICreateWindowsGuidance {
    static let limitation = "CLI-created registrations lack the required signed ARM64 guest driver payload; installation is blocked."
    static let setup = "For installation, create a new VM in the app with the ISO, signed payload and manifest. Existing CLI entries cannot be completed through the app or CLI."
    static let help = limitation + "\n      " + setup

    static func text(id: String) -> String {
        "No installation or guest boot was run.\n"
            + limitation + "\n" + setup + "\n"
            + "Inspect the saved registration: bridgevm app inspect \(id)\n"
    }
}
