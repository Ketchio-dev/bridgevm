import Foundation

extension HvfWindowsInstallPlan {
    static let unrepresentableDiskSizeMessage = "설치 디스크 크기를 파일 크기로 표현할 수 없습니다."

    /// FileHandle truncation ultimately uses signed off_t, not the full UInt64 range.
    /// This is a representation boundary, not a capacity or workload-fit guarantee.
    static func targetSizeBytes(diskGiB: Int) -> UInt64? {
        let bytesPerGiB: UInt64 = 1 << 30
        guard diskGiB > 0, let size = UInt64(exactly: diskGiB),
              size <= UInt64(Int64.max) / bytesPerGiB else { return nil }
        return size * bytesPerGiB
    }

    var freshTargetSizeBytes: UInt64? { Self.targetSizeBytes(diskGiB: request.diskGiB) }

    static func diskSizeError(_ diskGiB: Int) -> String? {
        guard diskGiB >= minimumDiskGiB else {
            return "디스크 크기는 최소 \(minimumDiskGiB) GiB여야 합니다."
        }
        return targetSizeBytes(diskGiB: diskGiB) == nil ? unrepresentableDiskSizeMessage : nil
    }
}
