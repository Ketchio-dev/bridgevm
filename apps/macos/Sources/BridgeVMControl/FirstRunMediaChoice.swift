import SwiftUI

struct FirstRunInstallerMedia: Identifiable {
    let url: URL
    var id: URL { url }
}

struct FirstRunWindowMedia {
    var installer: FirstRunInstallerMedia?
    var preparingDisk = false
}

private struct FirstRunWindowMediaKey: EnvironmentKey {
    static let defaultValue: Binding<FirstRunWindowMedia>? = nil
}

extension EnvironmentValues {
    var firstRunWindowMedia: Binding<FirstRunWindowMedia>? {
        get { self[FirstRunWindowMediaKey.self] }
        set { self[FirstRunWindowMediaKey.self] = newValue }
    }
}

enum FirstRunMediaChoice {
    case installer, disk

    @MainActor
    func droppedFile(_ urls: [URL]) -> URL? {
        guard let url = FileSelection.singleLocalURL(urls), VMLibrary.isReadableRegularFile(url.path) else { return nil }
        // RAW disks need no filename extension.
        let destination: Self = url.pathExtension.lowercased() == "iso" ? .installer : .disk
        return self == destination ? url : nil
    }

    var rejection: String {
        switch self {
        case .installer: return "Windows 설치 ISO 파일 한 개를 놓으세요."
        case .disk: return "Windows가 설치된 디스크 파일 한 개를 놓으세요. ISO는 ‘Windows 새로 설치’에 놓으세요."
        }
    }
}
