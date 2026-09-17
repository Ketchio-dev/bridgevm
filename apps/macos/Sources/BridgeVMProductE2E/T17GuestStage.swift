enum T17GuestStage: CaseIterable {
    case keyboardPointer, clipboard, folderShare, network, audio
    case firstShutdown, snapshotRestore, secondReady, secondShutdown

    var installStage: T17Stage {
        switch self {
        case .keyboardPointer: return .keyboardPointer
        case .clipboard: return .clipboard
        case .folderShare: return .folderShare
        case .network: return .network
        case .audio: return .audio
        case .firstShutdown: return .firstShutdown
        case .snapshotRestore: return .snapshotRestore
        case .secondReady: return .secondReady
        case .secondShutdown: return .secondShutdown
        }
    }
}
