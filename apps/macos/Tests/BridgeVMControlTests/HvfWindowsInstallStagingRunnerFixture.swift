import CryptoKit
import Darwin
import Foundation
import XCTest

enum HvfWindowsInstallStagingRunnerFixture {
    static func firmwareLogLine(_ log: String, runtime: URL) throws -> String {
        let prefix = "BRIDGEVM_AARCH64_UEFI_CODE="
        let lines = log.split(separator: "\n").map(String.init).filter { $0.hasPrefix(prefix) }
        XCTAssertEqual(lines.count, 1)
        let line = try XCTUnwrap(lines.first)
        let selected = URL(fileURLWithPath: String(line.dropFirst(prefix.count)))
        let expected = runtime.appendingPathComponent("firmware/edk2-aarch64-secure-code.fd")
        let observed = try XCTUnwrap(HvfWindowsInstallStagingFixture.status(selected))
        let trusted = try XCTUnwrap(HvfWindowsInstallStagingFixture.status(expected))
        XCTAssertEqual(observed.st_dev, trusted.st_dev)
        XCTAssertEqual(observed.st_ino, trusted.st_ino)
        return line
    }

    /// Copy the actual launcher/policy; the copied pin authenticates synthetic bytes only.
    static func installRunner(repo: URL, scratch: URL) throws -> URL {
        let checkout = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let scripts = repo.appendingPathComponent("scripts", isDirectory: true)
        let probe = repo.appendingPathComponent("target/release/examples/hvf_gic_boot_probe")
        let firmware = repo.appendingPathComponent("firmware/edk2-aarch64-secure-code.fd")
        let tools = scratch.appendingPathComponent("fake tools", isDirectory: true)
        for directory in [scripts, probe.deletingLastPathComponent(), firmware.deletingLastPathComponent(), tools] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try FileManager.default.copyItem(
            at: checkout.appendingPathComponent("scripts/run-hvf-windows-scripted-install.sh"),
            to: scripts.appendingPathComponent("run-hvf-windows-scripted-install.sh"))
        let payload = Data(repeating: 0x31, count: 3 * 1024 * 1024)
        try payload.write(to: firmware)
        let digest = SHA256.hash(data: payload).map { String(format: "%02x", $0) }.joined()
        let pin = "b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
        let policyName = "run-hvf-windows-scripted-install-policy.sh"
        let policy = try String(contentsOf: checkout.appendingPathComponent("scripts/" + policyName), encoding: .utf8)
        XCTAssertEqual(policy.components(separatedBy: pin).count, 2)
        try policy.replacingOccurrences(of: pin, with: digest)
            .write(to: scripts.appendingPathComponent(policyName), atomically: false, encoding: .utf8)
        try executable(probe, """
            for name in BRIDGEVM_NVME_DISK BRIDGEVM_NVME_DISK2 BRIDGEVM_AARCH64_UEFI_CODE BRIDGEVM_AARCH64_UEFI_VARS BRIDGEVM_RAMFB_DUMP_DIR; do
              printf '%s=%s\\n' "$name" "${!name}"
            done
            [[ -f "$BRIDGEVM_NVME_DISK2" && ! -L "$BRIDGEVM_NVME_DISK2" && -f "$BRIDGEVM_AARCH64_UEFI_VARS" ]]
            printf W | dd of="$BRIDGEVM_NVME_DISK2" bs=1 count=1 conv=notrunc 2>/dev/null
            printf P3 > "$BRIDGEVM_RAMFB_DUMP_DIR/frame 1.ppm"
            """)
        try executable(scripts.appendingPathComponent("verify-hvf-windows-install-target.sh"), """
            [[ $# == 2 && "$1" == --target && -f "$2" && ! -L "$2" ]] || exit 1
            printf 'verified %s\\n' "$2"
            """)
        try executable(tools.appendingPathComponent("codesign"), "echo '<key>com.apple.security.hypervisor</key>'")
        try executable(tools.appendingPathComponent("pgrep"), "exit 1")
        try executable(tools.appendingPathComponent("tmux"), "exit 1")
        return tools
    }

    private static func executable(_ url: URL, _ body: String) throws {
        try Data(("#!/bin/bash\nset -euo pipefail\n" + body + "\n").utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }
}
