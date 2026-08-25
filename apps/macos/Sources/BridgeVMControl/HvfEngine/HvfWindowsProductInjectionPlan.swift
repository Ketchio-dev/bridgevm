import Foundation

extension HvfWindowsInstallPlan {
    static let injectionResourcePaths = [
        "scripts/build-hvf-windows-kernel-policy-injector.sh",
        "scripts/build-hvf-windows-driver-injector.sh",
        "scripts/check-hvf-windows-viogpu3d-package.sh",
        "scripts/run-hvf-windows-installed-boot.sh",
        "scripts/run-hvf-windows-installed-boot-usage.sh",
        "scripts/run-hvf-windows-installed-boot-validation.sh",
        "scripts/run-hvf-windows-installed-boot-args.sh",
        "scripts/run-hvf-windows-installed-boot-runner.sh",
        "scripts/win-assets/winpeshl-inject.ini",
        "scripts/win-assets/bvinject.cmd",
    ]

    var importsExistingMedia: Bool { request.importedMedia == true }
    var driverSnapshotPath: String {
        "\(bundlePath)/\(HvfWindowsPreparedPackageSnapshot.bundleRelativePath)"
    }
    var injectionRootPath: String { "\(bundlePath)/metadata/hvf-injection" }
    var injectionWorkPath: String { "\(injectionRootPath)/work" }
    var injectorImagePath: String { "\(injectionWorkPath)/injector.raw" }
    var injectionEvidencePath: String { "\(injectionWorkPath)/evidence" }
    var retainedInjectionEvidencePath: String {
        "\(bundlePath)/logs/hvf/kernel-policy-injection"
    }

    func injectionValidationError(
        now: Date = Date(),
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor] =
            HvfWindowsKernelPolicyVerifier.productionTrustAnchors
    ) -> String? {
        guard request.injectViogpu3d else {
            return request.importedMedia == true
                ? "가져오기 주입 요청에 3D 주입 플래그가 없습니다." : nil
        }
        guard request.driverPackageDir == HvfWindowsPreparedPackageSnapshot.bundleRelativePath else {
            return HvfWindowsDriverPreflight.message(for: "provenance-snapshot-invalid")
        }
        return HvfWindowsDriverPreflight.inspect(
            packageDirectory: driverSnapshotPath, now: now,
            trustAnchors: trustAnchors).userMessage
    }

    func injectorBuildCommand() -> [String]? {
        guard request.injectViogpu3d,
              let wimlib = Self.wimlibCandidates.first(where: {
                  FileManager.default.isExecutableFile(atPath: $0)
              }) else { return nil }
        return [
            "/bin/bash", "scripts/build-hvf-windows-kernel-policy-injector.sh",
            "--iso", request.isoPath,
            "--package", driverSnapshotPath,
            "--out", injectorImagePath,
            "--wimlib", wimlib,
        ]
    }

    func injectionCommand() -> [String] {
        [
            "/bin/bash", "scripts/run-hvf-windows-installed-boot.sh",
            "--target", tmpTargetPath,
            "--vars", tmpVarsPath,
            "--placeholder-nsid1", injectorImagePath,
            "--evidence-dir", injectionEvidencePath,
            "--release", "--skip-build", "--watchdog-ms", "900000",
        ]
    }

    func injectionBootWasObserved() -> Bool {
        let targetStat = URL(fileURLWithPath: injectionEvidencePath)
            .appendingPathComponent("target-stat.txt")
        guard let text = try? String(contentsOf: targetStat, encoding: .utf8) else { return false }
        return text.split(whereSeparator: \.isNewline).contains("injector_boot_observed=true")
    }
}
