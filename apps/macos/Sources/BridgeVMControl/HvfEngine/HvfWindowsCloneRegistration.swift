import Foundation

extension VMConfig {
    /// The copy keeps the template's launch preferences except its shared folder, port forwards and
    /// automatic start: that folder was the user's selection for the source VM only, so a copy shares
    /// nothing until the user selects a folder for it, a Mac port can serve only one VM, and a copy
    /// that also started with BridgeVM would boot a second Windows at every launch.
    func windowsCloneRegistration(slug: String, name: String, bundle: URL) -> VMConfig {
        var cfg = self
        cfg.id = slug
        cfg.name = name
        cfg.displayName = name
        cfg.bundlePath = bundle.path
        cfg.diskPath = bundle.appendingPathComponent("disks/hvf-target.raw").path
        cfg.guestName = slug
        cfg.installPending = false
        cfg.sharedFolderHostPath = nil
        cfg.sharedFolderGuestPath = nil
        cfg.createSharedFolderOnStart = nil
        cfg.portForwards = nil
        cfg.startWithBridgeVM = nil
        return cfg
    }
}
