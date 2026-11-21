enum NativeCLICreateWindowsGuidance {
    static let limitation = "CLI-created registrations lack the required signed ARM64 guest driver payload; installation is blocked."
    static let setup = "For installation, create a new VM in the app with the ISO, signed payload and manifest. Existing CLI entries cannot be completed through the app or CLI."
    static let help = ["Without --fetch-drivers: " + limitation, setup,
        "With --fetch-drivers, the pinned Microsoft-signed Red Hat virtio-win storage,",
        "serial and network drivers (about 95 MB) are downloaded once from the official",
        "source, checked against their pinned SHA-256 and sealed with the VM, so the",
        "install command can complete it."].joined(separator: "\n      ")

    static func text(id: String) -> String {
        "No installation or guest boot was run.\n"
            + limitation + "\n" + setup + "\n"
            + "Inspect the saved registration: bridgevm app inspect \(id)\n"
    }

    static func sealedText(id: String) -> String {
        "No installation or guest boot was run.\n"
            + "Signed ARM64 guest drivers (Red Hat virtio-win) are sealed in the VM bundle.\n"
            + "Next: bridgevm app install \(id)\n"
    }
}
