import Foundation

extension VMConfig {
    /// The copy keeps the template's launch preferences except its shared folder and port
    /// forwards: that folder was the user's selection for the source VM only, so a copy shares
    /// nothing until the user selects a folder for it, and a Mac port can serve only one VM.
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
        return cfg
    }
}
