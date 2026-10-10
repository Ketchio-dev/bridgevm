import Foundation

extension HvfWindowsSnapshotCommand.Plan {
    func createArguments(destination: URL) throws -> [String] {
        // Resolve sizes through the same leased selection as create. Logical
        // originals intentionally retain their old bytes after a restore.
        let output = try HvfWindowsSnapshotCommand.invoke(executable, ["size", disk.path, vars.path])
        let quota = try HvfWindowsSnapshotQuota.parse(output)
        // Create rechecks this explicit ceiling under its own lease. A larger
        // intervening generation fails closed rather than expanding the quota.
        return ["create", disk.path, vars.path, destination.path, vmID, String(quota)]
    }
}

enum HvfWindowsSnapshotQuota {
    static func parse(_ output: String) throws -> UInt64 {
        let lines = output.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count == 3, lines[2].isEmpty else { throw invalid() }
        let disk = try bytes(lines[0], key: "disk_bytes")
        let vars = try bytes(lines[1], key: "vars_bytes")
        let sum = disk.addingReportingOverflow(vars)
        guard !sum.overflow, sum.partialValue > 0 else { throw invalid() }
        return sum.partialValue
    }

    private static func bytes(_ line: Substring, key: String) throws -> UInt64 {
        let fields = line.split(separator: " ", omittingEmptySubsequences: false)
        guard fields.count == 2, fields[0] == key, !fields[1].isEmpty,
              fields[1].utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }),
              let value = UInt64(fields[1]), String(value) == fields[1] else { throw invalid() }
        return value
    }

    private static func invalid() -> NSError {
        HvfWindowsSnapshotCommand.failure("selected snapshot quota is unavailable or invalid")
    }
}
