import Foundation

/// Copies the TPM workload and its TBS helper from the packaged app into the lane share
/// beside the other product E2E guest assets. Best effort: a missing, unsafe or
/// oversized asset is skipped, which only makes the TPM evidence `asset-missing`; it
/// never fails the lane.
enum T17TPMGuestAssets {
    static let names = ["bv-product-e2e-tpm.ps1", "bv-product-e2e-tpm.cs"]
    static let maximumBytes = 64 * 1024

    static func stage(appBundlePath: String, sharePath: String, fileManager: FileManager = .default) {
        let assets = URL(fileURLWithPath: appBundlePath).appendingPathComponent("Contents/Resources/scripts/win-assets")
        for name in names {
            let source = assets.appendingPathComponent(name)
            guard let attributes = try? fileManager.attributesOfItem(atPath: source.path),
                  attributes[.type] as? FileAttributeType == .typeRegular,
                  let size = (attributes[.size] as? NSNumber)?.intValue, size > 0, size <= maximumBytes else { continue }
            try? fileManager.copyItem(at: source, to: URL(fileURLWithPath: sharePath).appendingPathComponent(name))
        }
    }
}
