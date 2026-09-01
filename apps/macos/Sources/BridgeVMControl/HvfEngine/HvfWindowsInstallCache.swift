import Foundation

extension HvfWindowsInstallPlan {
    var sourceImageSHA256Path: String { sourceImagePath + ".sha256" }

    var sourceImageCacheCandidateExists: Bool {
        let manager = FileManager.default
        return manager.isReadableFile(atPath: sourceImagePath)
            && manager.isReadableFile(atPath: sourceImageSHA256Path)
    }

    func sourceImageCacheIsVerified() async -> Bool {
        guard sourceImageCacheCandidateExists else { return false }
        let imagePath = sourceImagePath
        let receiptPath = sourceImageSHA256Path
        return await Task.detached {
            guard let receipt = try? String(contentsOfFile: receiptPath, encoding: .utf8) else {
                return false
            }
            let expected = receipt.trimmingCharacters(in: .whitespacesAndNewlines)
            guard expected.count == 64,
                  expected.allSatisfy({ $0.isHexDigit && !$0.isUppercase }) else { return false }
            return HvfWindowsInstallCacheIdentity.sha256File(imagePath) == expected
        }.value
    }
}
